package com.garive.eng.kt.anthropic

import java.nio.file.Path
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class MessagesProtocolTest {
    private val root: Path = Path.of(requireNotNull(System.getProperty("garive.repo.root")))
    private val adapter: MessagesAdapter = MessagesAdapter(
        MessagesAdapterConfig(
            "https://compatible.example/messages",
            listOf(ProtocolHeader.create("x-api-key", "fixture-secret", true)),
            "x-protocol-version",
            "fixture-version",
        ),
    )

    @Test
    fun `configuration requires endpoint and version without leaking secrets`() {
        assertFails { MessagesAdapterConfig("/v1/messages", emptyList(), "x-version", "v1") }
        assertFails { MessagesAdapterConfig("https://example.test/messages", emptyList(), "x-version", "") }
        val debug = adapter.config.headers.single().toString()
        assertTrue("<redacted>" in debug); assertFalse("fixture-secret" in debug)
    }

    @Test
    fun `official request fixture is encoded exactly`() {
        val fixture = json("spec/fixtures/protocols/anthropic-messages/request.json")
        val messages = (fixture.getValue("messages") as JsonArray).map { element ->
            val turn = element.jsonObject
            Message(
                turn.getValue("role").jsonPrimitive.content,
                MessageContent.Blocks((turn.getValue("content") as JsonArray).map { block -> block.jsonObject }),
            )
        }
        val request = CreateMessageRequest(
            model = fixture.getValue("model").jsonPrimitive.content,
            maxTokens = fixture.getValue("max_tokens").jsonPrimitive.content.toULong(),
            messages = messages,
            stream = true,
            system = fixture["system"],
            tools = (fixture["tools"] as JsonArray).map { it.jsonObject },
            metadata = fixture["metadata"]?.jsonObject,
        )
        val prepared = adapter.prepare(request)
        assertEquals("https://compatible.example/messages", prepared.uri)
        assertEquals(fixture, Json.parseToJsonElement(prepared.body.decodeToString()))
        assertTrue(prepared.headers.any { it.name == "x-protocol-version" && it.value == "fixture-version" })
    }

    @Test
    fun `ordinary response preserves client and hosted block identity`() {
        val decoded = assertIs<DecodedResponse.Message>(adapter.decodeResponse(
            200, emptyList(), bytes("spec/fixtures/protocols/anthropic-messages/ordinary.json"),
        ))
        assertIs<OutputBlock.Text>(decoded.message.content[0])
        assertIs<OutputBlock.ToolUse>(decoded.message.content[1])
        val raw = decoded.message.raw
        val hosted = JsonObject(raw + ("content" to JsonArray(listOf(
            Json.parseToJsonElement("""{"type":"web_search_tool_result","tool_use_id":"srv_1"}"""),
        ))))
        val extended = assertIs<DecodedResponse.Message>(adapter.decodeResponse(200, emptyList(), hosted.toString().encodeToByteArray()))
        assertIs<OutputBlock.Extension>(extended.message.content.single())
    }

    @Test
    fun `error remains an unclassified protocol fact`() {
        val body = """{"type":"error","error":{"type":"future_capacity_error","message":"later"},"request_id":"req_1"}""".encodeToByteArray()
        val decoded = assertIs<DecodedResponse.Error>(adapter.decodeResponse(599, emptyList(), body))
        assertEquals(599, decoded.status); assertEquals("future_capacity_error", decoded.error.type)
    }

    @Test
    fun `stream is invariant under every byte split`() {
        val bytes = bytes("spec/fixtures/protocols/anthropic-messages/complete.sse")
        fun decode(first: ByteArray, second: ByteArray): List<StreamEventKind> {
            val decoder = MessagesStreamDecoder(); val events = decoder.push(first).toMutableList()
            events += decoder.push(second); decoder.finish(); return events.map(StreamEvent::kind)
        }
        val expected = decode(bytes, byteArrayOf())
        for (split in 0..bytes.size) assertEquals(expected, decode(bytes.copyOfRange(0, split), bytes.copyOfRange(split, bytes.size)), "split $split")
    }

    @Test
    fun `error terminal succeeds and truncation fails`() {
        val error = MessagesStreamDecoder()
        error.push(bytes("spec/fixtures/protocols/anthropic-messages/stream-error.sse")); error.finish()
        val truncated = MessagesStreamDecoder()
        truncated.push(bytes("spec/fixtures/protocols/anthropic-messages/truncated.sse"))
        assertFails { truncated.finish() }
    }

    private fun bytes(path: String): ByteArray = root.resolve(path).readBytes()
    private fun json(path: String): JsonObject = Json.parseToJsonElement(bytes(path).decodeToString()).jsonObject
}
