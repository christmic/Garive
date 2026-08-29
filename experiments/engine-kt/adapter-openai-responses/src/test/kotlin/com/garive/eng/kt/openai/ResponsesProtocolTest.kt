package com.garive.eng.kt.openai

import java.nio.file.Path
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.put

class ResponsesProtocolTest {
    private val root: Path = Path.of(requireNotNull(System.getProperty("garive.repo.root")))
    private val adapter: ResponsesAdapter = ResponsesAdapter(
        ResponsesAdapterConfig(
            "https://compatible.example/inference",
            listOf(ProtocolHeader.create("authorization", "Bearer fixture-secret", true)),
        ),
    )

    @Test
    fun `configuration is explicit and secrets are redacted`() {
        assertFails { ResponsesAdapterConfig("/v1/responses", emptyList()) }
        assertFails {
            ResponsesAdapterConfig(
                "https://example.test/responses",
                listOf(ProtocolHeader.create("accept", "application/json", false)),
            )
        }
        val debug = adapter.config.headers.single().toString()
        assertTrue("<redacted>" in debug)
        assertFalse("fixture-secret" in debug)
    }

    @Test
    fun `official request fixture is encoded`() {
        val fixture = json("spec/fixtures/providers/openai/responses/request.json")
        val request = CreateResponseRequest(
            model = fixture.getValue("model").toString().trim('"'),
            input = ResponseInput.Items(fixture.getValue("input").let { it as kotlinx.serialization.json.JsonArray }.map { it as JsonObject }),
            stream = true,
            maxOutputTokens = fixture["max_output_tokens"]?.toString()?.toULong(),
            tools = (fixture["tools"] as? kotlinx.serialization.json.JsonArray)?.map { it as JsonObject } ?: emptyList(),
            metadata = (fixture["metadata"] as? JsonObject)?.mapValues { it.value.toString().trim('"') } ?: emptyMap(),
            extensions = JsonObject(fixture.filterKeys { it == "store" }),
        )
        val prepared = adapter.prepare(request)
        assertEquals("POST", prepared.method)
        assertEquals("https://compatible.example/inference", prepared.uri)
        assertEquals(Json.parseToJsonElement(fixture.toString()), Json.parseToJsonElement(prepared.body.decodeToString()))
    }

    @Test
    fun `ordinary and hosted outputs keep protocol identity`() {
        val fixture = bytes("spec/fixtures/providers/openai/responses/ordinary.json")
        val decoded = assertIs<DecodedResponse.Response>(adapter.decodeResponse(200, emptyList(), fixture))
        assertTrue(decoded.response.output.isNotEmpty())
        val value = Json.parseToJsonElement(fixture.decodeToString()) as JsonObject
        val modified = JsonObject(value + ("output" to kotlinx.serialization.json.JsonArray(listOf(
            kotlinx.serialization.json.buildJsonObject { put("type", "web_search_call"); put("id", "hosted_1") },
        ))))
        val hosted = assertIs<DecodedResponse.Response>(adapter.decodeResponse(200, emptyList(), modified.toString().encodeToByteArray()))
        assertIs<ResponseOutputItem.Extension>(hosted.response.output.single())
    }

    @Test
    fun `incremental stream is invariant under every byte split`() {
        val bytes = bytes("spec/fixtures/providers/openai/responses/complete.sse")
        fun decode(first: ByteArray, second: ByteArray): List<String> {
            val decoder = ResponsesStreamDecoder()
            val events = decoder.push(first).toMutableList()
            events += decoder.push(second)
            decoder.finish()
            return events.map { it.type }
        }
        val expected = decode(bytes, byteArrayOf())
        for (split in 0..bytes.size) {
            assertEquals(expected, decode(bytes.copyOfRange(0, split), bytes.copyOfRange(split, bytes.size)), "split $split")
        }
    }

    @Test
    fun `truncated stream fails closed`() {
        val decoder = ResponsesStreamDecoder()
        decoder.push(bytes("spec/fixtures/providers/openai/responses/truncated.sse"))
        assertFails { decoder.finish() }
    }

    private fun bytes(path: String): ByteArray = root.resolve(path).readBytes()
    private fun json(path: String): JsonObject = Json.parseToJsonElement(bytes(path).decodeToString()) as JsonObject
}
