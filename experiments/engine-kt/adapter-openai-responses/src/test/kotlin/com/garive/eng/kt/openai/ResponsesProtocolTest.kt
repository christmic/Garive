package com.garive.eng.kt.openai

import java.nio.file.Path
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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
        val fixture = json("spec/fixtures/protocols/openai-responses/request.json")
        val request = CreateResponseRequest(
            model = fixture.getValue("model").toString().trim('"'),
            input = ResponseInput.Items(listOf(InputItem.Message(
                MessageRole.USER,
                listOf(InputContent.Text("hello")),
            ))),
            stream = true,
            maxOutputTokens = fixture["max_output_tokens"]?.toString()?.toULong(),
            tools = listOf(FunctionTool(
                name = "weather",
                description = "Lookup weather",
                parameters = ((fixture["tools"] as kotlinx.serialization.json.JsonArray)[0] as JsonObject)
                    .getValue("parameters") as JsonObject,
                strict = true,
            )),
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
        val fixture = bytes("spec/fixtures/protocols/openai-responses/ordinary.json")
        val decoded = assertIs<DecodedResponse.Response>(adapter.decodeResponse(200, emptyList(), fixture))
        val message = assertIs<ResponseOutputItem.Message>(decoded.response.output[0])
        assertIs<OutputContent.Text>(message.content.single())
        val call = assertIs<ResponseOutputItem.FunctionCall>(decoded.response.output[1])
        assertEquals("call_weather", call.callId)
        assertEquals(17uL, decoded.response.usage?.totalTokens)
        val value = Json.parseToJsonElement(fixture.decodeToString()) as JsonObject
        val modified = JsonObject(value + ("output" to kotlinx.serialization.json.JsonArray(listOf(
            kotlinx.serialization.json.buildJsonObject { put("type", "web_search_call"); put("id", "hosted_1") },
        ))))
        val hosted = assertIs<DecodedResponse.Response>(adapter.decodeResponse(200, emptyList(), modified.toString().encodeToByteArray()))
        assertIs<ResponseOutputItem.Extension>(hosted.response.output.single())
    }

    @Test
    fun `shared error matrix remains unclassified protocol data`() {
        val cases = json("spec/fixtures/protocols/openai-responses/errors.json").getValue("cases") as JsonArray
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
    fun `malformed known output and usage fail closed`() {
        val value = json("spec/fixtures/protocols/openai-responses/ordinary.json")
        val invalidUsage = JsonObject(value + ("usage" to JsonObject(value.getValue("usage").jsonObject + ("total_tokens" to kotlinx.serialization.json.JsonPrimitive(18)))))
        assertFails { adapter.decodeResponse(200, emptyList(), invalidUsage.toString().encodeToByteArray()) }
        val invalidItem = JsonObject(value + ("output" to JsonArray(listOf(
            kotlinx.serialization.json.buildJsonObject { put("type", "message"); put("id", ""); put("role", "assistant"); put("status", "completed"); put("content", JsonArray(emptyList())) },
        ))))
        assertFails { adapter.decodeResponse(200, emptyList(), invalidItem.toString().encodeToByteArray()) }
    }

    @Test
    fun `portable request unions encode as official shapes`() {
        val request = CreateResponseRequest(
            model = "model",
            input = ResponseInput.Items(listOf(InputItem.FunctionCallOutput(
                "call_1",
                FunctionOutput.Content(listOf(InputContent.Image(fileId = "file_1", detail = ImageDetail.LOW))),
                ItemStatus.COMPLETED,
            ))),
            stream = false,
            toolChoice = ToolChoice.Mode(ToolChoiceMode.REQUIRED),
            text = ResponseTextConfig(TextFormat.JsonSchema(
                "answer", schema = kotlinx.serialization.json.buildJsonObject { put("type", "object") }, strict = true,
            )),
            reasoning = ReasoningConfig(ReasoningEffort.XHIGH, ReasoningSummary.DETAILED),
        )
        val value = Json.parseToJsonElement(adapter.prepare(request).body.decodeToString()).jsonObject
        assertEquals("file_1", value.getValue("input").let { it as JsonArray }[0].jsonObject
            .getValue("output").let { it as JsonArray }[0].jsonObject.getValue("file_id").jsonPrimitive.content)
        assertEquals("required", value.getValue("tool_choice").jsonPrimitive.content)
        assertEquals("json_schema", value.getValue("text").jsonObject.getValue("format").jsonObject.getValue("type").jsonPrimitive.content)
        assertEquals("xhigh", value.getValue("reasoning").jsonObject.getValue("effort").jsonPrimitive.content)
        assertFailsWith<ResponsesProtocolException> {
            adapter.prepare(request.copy(input = ResponseInput.Items(listOf(
                InputItem.FunctionCallOutput("call_1", FunctionOutput.Content(emptyList())),
            ))))
        }
    }

    @Test
    fun `incremental stream is invariant under every byte split`() {
        val bytes = bytes("spec/fixtures/protocols/openai-responses/complete.sse")
        fun decode(first: ByteArray, second: ByteArray): List<String> {
            val decoder = ResponsesStreamDecoder()
            val events = decoder.push(first).toMutableList()
            events += decoder.push(second)
            decoder.finish()
            return events.map { it.discriminator }
        }
        val expected = decode(bytes, byteArrayOf())
        for (split in 0..bytes.size) {
            assertEquals(expected, decode(bytes.copyOfRange(0, split), bytes.copyOfRange(split, bytes.size)), "split $split")
        }
    }

    @Test
    fun `truncated stream fails closed`() {
        val decoder = ResponsesStreamDecoder()
        decoder.push(bytes("spec/fixtures/protocols/openai-responses/truncated.sse"))
        val failure = assertFailsWith<ResponsesProtocolException> { decoder.finish() }
        assertEquals(ResponsesProtocolError.TRUNCATED_STREAM, failure.error)
    }

    private fun bytes(path: String): ByteArray = root.resolve(path).readBytes()
    private fun json(path: String): JsonObject = Json.parseToJsonElement(bytes(path).decodeToString()) as JsonObject
}
