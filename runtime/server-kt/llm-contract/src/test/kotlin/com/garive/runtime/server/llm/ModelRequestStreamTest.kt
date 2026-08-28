package com.garive.runtime.server.llm

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class ModelRequestStreamTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/agent/model-request-stream.json").readText()).jsonObject
    }

    private fun request() = ModelRequest(
        requestId = ModelRequestId("request-1"),
        targetId = ModelTargetId("primary"),
        requiredCapabilities = listOf(ModelCapability.TEXT, ModelCapability.STREAMING),
        inputItems = listOf(ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text("hello")))),
        tools = listOf(ToolDescriptor("lookup", "look up", "1", "{}", strict = true)),
        output = ModelOutputSettings(100u, TextMode.Plain, reasoningVisibility = false),
        traceMetadata = listOf("trace" to "one"),
    )

    @Test
    fun `Kotlin consumes every request case`() {
        val cases = document.getValue("request_cases").jsonArray
        assertEquals(5, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val base = request()
            val value = when (case.text("mutation")) {
                "none" -> base
                "empty-request-id" -> base.copy(requestId = ModelRequestId(""))
                "duplicate-capability" -> base.copy(requiredCapabilities = base.requiredCapabilities + ModelCapability.TEXT)
                "duplicate-tool" -> base.copy(tools = base.tools + base.tools.first())
                "zero-output-limit" -> base.copy(output = base.output.copy(maxOutputTokens = 0u))
                else -> error("unknown mutation")
            }
            assertEquals(case.text("expected"), value.validate()?.code ?: "ok", case.text("name"))
        }
    }

    @Test
    fun `Kotlin consumes every stream case`() {
        val cases = document.getValue("stream_cases").jsonArray
        assertEquals(6, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val validator = StreamValidator()
            var result: StreamInvariantError? = null
            for (encoded in case.getValue("events").jsonArray) {
                result = validator.accept(event(encoded.jsonPrimitive.content))
                if (result != null) break
            }
            assertEquals(case.text("expected"), result?.code ?: "ok", case.text("name"))
        }
    }

    private fun event(encoded: String): ModelStreamEvent {
        val parts = encoded.split(':')
        val index = parts.getOrNull(1)?.toUInt() ?: 0u
        return when (parts[0]) {
            "start" -> ModelStreamEvent.OutputItemStarted(index, kind(parts[2]))
            "text" -> ModelStreamEvent.TextDelta(index, parts[2])
            "refusal" -> ModelStreamEvent.RefusalDelta(index, parts[2])
            "reasoning" -> ModelStreamEvent.ReasoningDelta(index, parts[2])
            "complete" -> ModelStreamEvent.OutputItemCompleted(index, item(parts[2]))
            "usage" -> ModelStreamEvent.UsageUpdated(
                ModelUsage(TokenCount.Known(1u), TokenCount.Known(1u), source = UsageSource.PROVIDER_REPORTED),
            )
            else -> error("unknown event $encoded")
        }
    }

    private fun kind(value: String): ModelOutputKind = when (value) {
        "text" -> ModelOutputKind.Text
        "refusal" -> ModelOutputKind.Refusal
        "reasoning" -> ModelOutputKind.Reasoning
        else -> error("unknown kind $value")
    }

    private fun item(value: String): ModelItem = when (value) {
        "text" -> ModelItem.Text("a")
        "refusal" -> ModelItem.Refusal("no")
        "reasoning" -> ModelItem.Reasoning(ReasoningContent.ModelVisible("r"))
        else -> error("unknown item $value")
    }

    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
}
