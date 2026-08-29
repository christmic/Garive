package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.MessageContent
import com.garive.eng.kt.anthropic.SystemPrompt
import com.garive.eng.kt.llm.ModelCapability
import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelOutputSettings
import com.garive.eng.kt.llm.ModelRequest
import com.garive.eng.kt.llm.ModelRequestId
import com.garive.eng.kt.llm.ModelRole
import com.garive.eng.kt.llm.ModelTargetId
import com.garive.eng.kt.llm.TextMode
import com.garive.eng.kt.llm.ToolDescriptor
import com.garive.eng.kt.openai.ResponseInput
import com.garive.eng.kt.openai.ResponseEnvelope
import com.garive.eng.kt.openai.TextFormat
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

public class RequestMappingTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"), "spec/fixtures/providers/compatible-mapping-v1.json")
            .toFile().readText(),
    ).jsonObject

    @Test
    public fun `shared request cases map both portable protocols`(): Unit {
        val cases = fixture["request_cases"]!!.jsonArray
        val responsesExpected = cases[0].jsonObject["expected"]!!.jsonObject
        val responses = mapResponsesRequest(
            responsesDeployment(),
            request(target = "responses-target", metadata = listOf("trace" to "fixture"), schema = true),
        )
        assertEquals(responsesExpected["model"]!!.jsonPrimitive.content, responses.model)
        assertEquals(responsesExpected["max_output_tokens"]!!.jsonPrimitive.content.toULong(), responses.maxOutputTokens)
        assertEquals(true, responses.stream)
        assertIs<ResponseInput.Items>(responses.input)
        assertIs<TextFormat.JsonSchema>(responses.text!!.format)

        val messagesExpected = cases[1].jsonObject["expected"]!!.jsonObject
        val messages = mapMessagesRequest(messagesDeployment(), request(target = "messages-target"))
        assertEquals(messagesExpected["model"]!!.jsonPrimitive.content, messages.model)
        assertEquals(messagesExpected["max_output_tokens"]!!.jsonPrimitive.content.toULong(), messages.maxTokens)
        assertEquals(2, (messages.system as SystemPrompt.Blocks).value.size)
        assertEquals(2, messages.messages.size)
        assertIs<MessageContent.Blocks>(messages.messages[1].content)
    }

    @Test
    public fun `target capability late instruction and metadata fail closed`(): Unit {
        val mismatch = assertFailsWith<CompatibleProviderException> {
            mapResponsesRequest(responsesDeployment(), request(target = "wrong", metadata = listOf("trace" to "x"), schema = true))
        }
        assertEquals(CompatibleProviderError.TARGET_MISMATCH, mismatch.error)

        val late = request(target = "messages-target").copy(
            inputItems = request(target = "messages-target").inputItems +
                ModelInputItem.Message(ModelRole.DEVELOPER, listOf(ModelInputContent.Text("late"))),
        )
        assertEquals(
            CompatibleProviderError.UNSUPPORTED_INPUT,
            assertFailsWith<CompatibleProviderException> { mapMessagesRequest(messagesDeployment(), late) }.error,
        )
        assertEquals(
            CompatibleProviderError.UNSUPPORTED_METADATA,
            assertFailsWith<CompatibleProviderException> {
                mapMessagesRequest(messagesDeployment(), request("messages-target", listOf("trace" to "x")))
            }.error,
        )
    }

    @Test
    public fun `every shared failure case returns its stable code`(): Unit {
        fixture["failure_cases"]!!.jsonArray.forEach { element ->
            val case = element.jsonObject
            val name = case["name"]!!.jsonPrimitive.content
            val exception = assertFailsWith<CompatibleProviderException> {
                when (name) {
                    "target-mismatch" -> mapResponsesRequest(responsesDeployment(), request("wrong", schema = true))
                    "unsupported-capability" -> mapResponsesRequest(
                        responsesDeployment(),
                        request("responses-target", schema = true).copy(
                            requiredCapabilities = request("responses-target").requiredCapabilities + ModelCapability.VISION,
                        ),
                    )
                    "invalid-tool-schema" -> mapResponsesRequest(
                        responsesDeployment(),
                        request("responses-target", schema = true).copy(
                            tools = listOf(ToolDescriptor("lookup", "Lookup.", "1", "[]", true)),
                        ),
                    )
                    "messages-late-instruction" -> mapMessagesRequest(
                        messagesDeployment(),
                        request("messages-target").copy(inputItems = request("messages-target").inputItems +
                            ModelInputItem.Message(ModelRole.DEVELOPER, listOf(ModelInputContent.Text("late")))),
                    )
                    "messages-metadata" -> mapMessagesRequest(
                        messagesDeployment(),
                        request("messages-target", listOf("trace" to "x")),
                    )
                    "reasoning-without-profile" -> mapResponsesRequest(
                        responsesDeployment(),
                        request("responses-target", schema = true).copy(
                            output = request("responses-target", schema = true).output.copy(reasoningVisibility = true),
                        ),
                    )
                    "unadmitted-extension" -> normalizeResponses(extensionResponse(), false)
                    "messages-missing-output-limit" -> mapMessagesRequest(
                        messagesDeployment().copy(defaultMaxOutputTokens = null),
                        request("messages-target"),
                    )
                    else -> error("unhandled shared failure $name")
                }
            }
            assertEquals(case["code"]!!.jsonPrimitive.content, exception.error.code)
        }
    }

    private fun request(
        target: String,
        metadata: List<Pair<String, String>> = emptyList(),
        schema: Boolean = false,
    ): ModelRequest = ModelRequest(
        requestId = ModelRequestId("request"),
        targetId = ModelTargetId(target),
        requiredCapabilities = listOf(
            ModelCapability.TEXT,
            ModelCapability.TOOLS,
            ModelCapability.JSON_OUTPUT,
            ModelCapability.STREAMING,
        ),
        inputItems = listOf(
            ModelInputItem.Message(ModelRole.SYSTEM, listOf(ModelInputContent.Text("policy"))),
            ModelInputItem.Message(ModelRole.DEVELOPER, listOf(ModelInputContent.Text("developer"))),
            ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text("question"))),
            ModelInputItem.ToolObservation("call-0", "{\"ok\":true}"),
        ),
        tools = listOf(ToolDescriptor("lookup", "Lookup.", "1", "{\"type\":\"object\"}", true)),
        output = ModelOutputSettings(
            maxOutputTokens = if (target == "responses-target") 128u else null,
            textMode = if (schema) TextMode.JsonSchema("{\"type\":\"object\"}") else TextMode.JsonObject,
            reasoningVisibility = false,
        ),
        traceMetadata = metadata,
    )

    private fun responsesDeployment(): ResponsesDeployment = ResponsesDeployment(
        targetId = "responses-target",
        modelId = "compatible-responses-model",
        capabilities = setOf(ModelCapability.TEXT, ModelCapability.TOOLS, ModelCapability.JSON_OUTPUT, ModelCapability.STREAMING),
    )

    private fun messagesDeployment(): MessagesDeployment = MessagesDeployment(
        targetId = "messages-target",
        modelId = "compatible-messages-model",
        capabilities = setOf(ModelCapability.TEXT, ModelCapability.TOOLS, ModelCapability.JSON_OUTPUT, ModelCapability.STREAMING),
        defaultMaxOutputTokens = 512u,
    )

    private fun extensionResponse(): ResponseEnvelope = ResponseEnvelope.parse(buildJsonObject {
        put("id", "response"); put("created_at", 1.0); put("error", JsonNull); put("incomplete_details", JsonNull)
        put("instructions", JsonNull); put("metadata", JsonNull); put("model", "model"); put("object", "response")
        put("output", JsonArray(listOf(buildJsonObject { put("type", "hosted_tool_call"); put("id", "hosted") })))
        put("parallel_tool_calls", false); put("temperature", JsonNull); put("tool_choice", "auto")
        put("tools", JsonArray(emptyList())); put("top_p", JsonNull); put("status", "completed"); put("usage", JsonNull)
    })
}
