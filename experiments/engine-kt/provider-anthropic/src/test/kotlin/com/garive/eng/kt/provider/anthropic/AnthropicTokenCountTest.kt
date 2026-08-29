package com.garive.eng.kt.provider.anthropic

import com.garive.eng.kt.anthropic.ContentBlock
import com.garive.eng.kt.anthropic.CreateMessageRequest
import com.garive.eng.kt.anthropic.JsonOutputFormat
import com.garive.eng.kt.anthropic.Message
import com.garive.eng.kt.anthropic.MessageContent
import com.garive.eng.kt.anthropic.MessageRole
import com.garive.eng.kt.anthropic.Metadata
import com.garive.eng.kt.anthropic.OutputConfig
import com.garive.eng.kt.anthropic.SystemPrompt
import com.garive.eng.kt.anthropic.ThinkingConfig
import com.garive.eng.kt.anthropic.Tool
import com.garive.eng.kt.anthropic.ToolChoice
import com.garive.eng.kt.provider.profile.ConnectionInput
import com.garive.eng.kt.provider.profile.EndpointSelection
import com.garive.eng.kt.provider.profile.ExplicitHeader
import com.garive.eng.kt.provider.profile.SecretValue
import com.garive.eng.kt.provider.profile.VendorProfileError
import com.garive.eng.kt.provider.profile.VendorProfileException
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

public class AnthropicTokenCountTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"), "spec/fixtures/providers/anthropic-token-count-v1.json")
            .toFile().readText(),
    ).jsonObject

    @Test
    public fun `shared projection and response shapes are exact`(): Unit {
        val cases = fixture["projection_cases"]!!.jsonArray
        assertEquals(1, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val projected = projectTokenCountRequest(createRequest(case["create_request"]!!.jsonObject))
            val profile = profile()
            val exchange = profile.prepare(projected)
            assertEquals(Constants.METHOD_POST, exchange.method)
            assertEquals(fixture["profile"]!!.jsonObject["default_endpoint"]!!.jsonPrimitive.content, exchange.uri)
            assertEquals(case["expected_count_request"]!!.jsonObject, Json.parseToJsonElement(exchange.body.decodeToString()))
        }

        fixture["response_cases"]!!.jsonArray.forEach { element ->
            val case = element.jsonObject
            val count = decodeTokenCount(case["body"]!!.toString().encodeToByteArray())
            assertEquals(case["expected"]!!.jsonPrimitive.content.toULong(), count.inputTokens)
        }
    }

    @Test
    public fun `shared failures and duplicate response fail closed`(): Unit {
        val create = createRequest(fixture["projection_cases"]!!.jsonArray[0].jsonObject["create_request"]!!.jsonObject)
        val extended = create.copy(extensions = JsonObject(mapOf("hosted" to JsonObject(emptyMap()))))
        assertTokenError("request-extension") { projectTokenCountRequest(extended) }

        fixture["failure_cases"]!!.jsonArray.map { it.jsonObject }
            .filter { it["body"] != null }
            .forEach { case -> assertTokenError(case["name"]!!.jsonPrimitive.content) {
                decodeTokenCount(case["body"]!!.toString().encodeToByteArray())
            } }
        assertFailsWith<AnthropicTokenCountException> {
            decodeTokenCount("{\"input_tokens\":1,\"input_tokens\":2}".encodeToByteArray())
        }
        assertFailsWith<AnthropicTokenCountException> { decodeTokenCount(byteArrayOf(0xC3.toByte())) }
    }

    @Test
    public fun `explicit profile is exact redacted and validates configuration`(): Unit {
        val profile = profile()
        assertFalse(profile.toString().contains("fixture-secret"))
        assertEquals(
            fixture["profile"]!!.jsonObject["protocol_version"]!!.jsonPrimitive.content,
            profile.headers.single { it.name == "anthropic-version" }.value,
        )

        assertProfileError("explicit-relative-endpoint", VendorProfileError.INVALID_ENDPOINT,
            ConnectionInput(EndpointSelection.Explicit("/count_tokens"), secret(), emptyList()))
        assertProfileError("reserved-version-header", VendorProfileError.RESERVED_HEADER,
            ConnectionInput(EndpointSelection.Default, secret(), listOf(
                ExplicitHeader.create("Anthropic-Version", "caller", false),
            )))
    }

    private fun profile(): AnthropicTokenCountProfile = buildAnthropicTokenCountProfile(
        ConnectionInput(EndpointSelection.Default, secret(), listOf(
            ExplicitHeader.create("x-trace", "trace-1", false),
        )),
    )

    private fun secret(): SecretValue = SecretValue.create("fixture-secret")

    private fun assertTokenError(name: String, block: () -> Unit): Unit {
        val case = failure(name)
        val error = assertFailsWith<AnthropicTokenCountException>(block = block)
        assertEquals(case["code"]!!.jsonPrimitive.content, error.error.code)
    }

    private fun assertProfileError(name: String, expected: VendorProfileError, input: ConnectionInput): Unit {
        val case = failure(name)
        val error = assertFailsWith<VendorProfileException> { buildAnthropicTokenCountProfile(input) }
        assertEquals(expected, error.error)
        assertEquals(case["code"]!!.jsonPrimitive.content, error.error.code)
    }

    private fun failure(name: String): JsonObject = fixture["failure_cases"]!!.jsonArray
        .map { it.jsonObject }.single { it["name"]!!.jsonPrimitive.content == name }

    private fun createRequest(value: JsonObject): CreateMessageRequest = CreateMessageRequest(
        model = value["model"]!!.jsonPrimitive.content,
        maxTokens = value["max_tokens"]!!.jsonPrimitive.content.toULong(),
        messages = value["messages"]!!.jsonArray.map { message ->
            val item = message.jsonObject
            Message(
                if (item["role"]!!.jsonPrimitive.content == "user") MessageRole.USER else MessageRole.ASSISTANT,
                MessageContent.Blocks(textBlocks(item["content"]!!.jsonArray)),
            )
        },
        stream = value["stream"]!!.jsonPrimitive.boolean,
        system = SystemPrompt.Blocks(textBlocks(value["system"]!!.jsonArray)),
        stopSequences = value["stop_sequences"]!!.jsonArray.map { it.jsonPrimitive.content },
        temperature = value["temperature"]!!.jsonPrimitive.double,
        topP = value["top_p"]!!.jsonPrimitive.double,
        topK = value["top_k"]!!.jsonPrimitive.content.toULong(),
        tools = value["tools"]!!.jsonArray.map { element ->
            val tool = element.jsonObject
            Tool(tool["name"]!!.jsonPrimitive.content, tool["input_schema"]!!.jsonObject,
                tool["description"]!!.jsonPrimitive.content, tool["strict"]!!.jsonPrimitive.boolean)
        },
        toolChoice = ToolChoice.Auto(),
        outputConfig = OutputConfig(format = JsonOutputFormat(
            value["output_config"]!!.jsonObject["format"]!!.jsonObject["schema"]!!.jsonObject,
        )),
        thinking = ThinkingConfig.Enabled(
            value["thinking"]!!.jsonObject["budget_tokens"]!!.jsonPrimitive.long.toULong(),
        ),
        metadata = Metadata(value["metadata"]!!.jsonObject["user_id"]!!.jsonPrimitive.content),
    )

    private fun textBlocks(values: JsonArray): List<ContentBlock.Text> = values.map { element ->
        ContentBlock.Text(element.jsonObject["text"]!!.jsonPrimitive.content)
    }
}
