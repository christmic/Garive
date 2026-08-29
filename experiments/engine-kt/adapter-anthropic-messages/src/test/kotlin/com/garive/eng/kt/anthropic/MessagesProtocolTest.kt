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
        val messages = listOf(Message(
            MessageRole.USER,
            MessageContent.Blocks(listOf(ContentBlock.Text("hello"))),
        ))
        val fixtureTool = (fixture["tools"] as JsonArray)[0].jsonObject
        val request = CreateMessageRequest(
            model = fixture.getValue("model").jsonPrimitive.content,
            maxTokens = fixture.getValue("max_tokens").jsonPrimitive.content.toULong(),
            messages = messages,
            stream = true,
            system = SystemPrompt.Blocks(listOf(ContentBlock.Text("be concise"))),
            tools = listOf(Tool(
                name = "weather",
                inputSchema = fixtureTool.getValue("input_schema").jsonObject,
                description = "Lookup weather",
            )),
            metadata = Metadata("fixture"),
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
        assertEquals("hello back", assertIs<OutputBlock.Text>(decoded.message.content[0]).text)
        assertEquals("weather", assertIs<OutputBlock.ToolUse>(decoded.message.content[1]).name)
        assertEquals(8uL, decoded.message.usage.inputTokens)
        val raw = decoded.message.raw
        val hosted = JsonObject(raw + ("content" to JsonArray(listOf(
            Json.parseToJsonElement("""{"type":"web_search_tool_result","tool_use_id":"srv_1"}"""),
        ))))
        val extended = assertIs<DecodedResponse.Message>(adapter.decodeResponse(200, emptyList(), hosted.toString().encodeToByteArray()))
        assertIs<OutputBlock.Extension>(extended.message.content.single())
    }

    @Test
    fun `shared error matrix remains unclassified protocol data`() {
        val cases = json("spec/fixtures/protocols/anthropic-messages/errors.json").getValue("cases") as JsonArray
        cases.forEach { element ->
            val case = element.jsonObject
            val status = case.getValue("status").jsonPrimitive.content.toInt()
            val decoded = assertIs<DecodedResponse.Error>(adapter.decodeResponse(
                status, emptyList(), case.getValue("body").toString().encodeToByteArray(),
            ))
            assertEquals(status, decoded.status)
            assertEquals(case.getValue("expected_error_type").jsonPrimitive.content, decoded.error.type)
        }
    }

    @Test
    fun `error remains an unclassified protocol fact`() {
        val body = """{"type":"error","error":{"type":"future_capacity_error","message":"later"},"request_id":"req_1"}""".encodeToByteArray()
        val decoded = assertIs<DecodedResponse.Error>(adapter.decodeResponse(599, emptyList(), body))
        assertEquals(599, decoded.status); assertEquals("future_capacity_error", decoded.error.type)
    }

    @Test
    fun `portable source output thinking and choice unions are typed`() {
        val request = CreateMessageRequest(
            model = "model",
            maxTokens = 2_048u,
            messages = listOf(Message(MessageRole.USER, MessageContent.Blocks(listOf(
                ContentBlock.Image(
                    ImageSource.Base64(ImageMediaType.PNG, "aGVsbG8="),
                    CacheControl(CacheTtl.ONE_HOUR),
                ),
                ContentBlock.Document(
                    DocumentSource.Text("document"),
                    citations = CitationsConfig(true),
                ),
            )))),
            stream = false,
            toolChoice = ToolChoice.None,
            thinking = ThinkingConfig.Enabled(1_024u, ThinkingDisplay.OMITTED),
            outputConfig = OutputConfig(
                effort = Effort.XHIGH,
                format = JsonOutputFormat(inlineJson("""{"type":"object"}""")),
            ),
        )
        val value = Json.parseToJsonElement(adapter.prepare(request).body.decodeToString()).jsonObject
        assertEquals("image/png", value.getValue("messages").let { it as JsonArray }[0].jsonObject
            .getValue("content").let { it as JsonArray }[0].jsonObject
            .getValue("source").jsonObject.getValue("media_type").jsonPrimitive.content)
        assertEquals("none", value.getValue("tool_choice").jsonObject.getValue("type").jsonPrimitive.content)
        assertEquals("omitted", value.getValue("thinking").jsonObject.getValue("display").jsonPrimitive.content)
        assertEquals("json_schema", value.getValue("output_config").jsonObject
            .getValue("format").jsonObject.getValue("type").jsonPrimitive.content)
    }

    @Test
    fun `invalid source thinking and tool-result block fail before transport`() {
        val invalidSource = CreateMessageRequest(
            "model", 2_048u,
            listOf(Message(MessageRole.USER, MessageContent.Blocks(listOf(
                ContentBlock.Image(ImageSource.Url("")),
            )))), false,
        )
        assertFails { adapter.prepare(invalidSource) }
        val invalidThinking = invalidSource.copy(
            messages = listOf(Message(MessageRole.USER, MessageContent.Text("hello"))),
            maxTokens = 1_024u,
            thinking = ThinkingConfig.Enabled(1_024u),
        )
        assertFails { adapter.prepare(invalidThinking) }
        val invalidResult = invalidThinking.copy(
            maxTokens = 2_048u,
            thinking = null,
            messages = listOf(Message(MessageRole.USER, MessageContent.Blocks(listOf(
                ContentBlock.ToolResult("call", ToolResultContent.Blocks(listOf(
                    ContentBlock.ToolUse("nested", "bad", JsonObject(emptyMap())),
                ))),
            )))),
        )
        assertFails { adapter.prepare(invalidResult) }
    }

    @Test
    fun `stream is invariant under every byte split`() {
        val bytes = bytes("spec/fixtures/protocols/anthropic-messages/complete.sse")
        fun decode(first: ByteArray, second: ByteArray): List<String> {
            val decoder = MessagesStreamDecoder(); val events = decoder.push(first).toMutableList()
            events += decoder.push(second); decoder.finish(); return events.map(StreamEvent::discriminator)
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

    private fun inlineJson(value: String): JsonObject =
        Json.parseToJsonElement(value).jsonObject
}
