package com.garive.runtime.server.openai

import com.garive.runtime.server.llm.*
import java.nio.file.Path
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json

class OpenAiResponsesCodecTest {
    private val root = Path.of(System.getProperty("garive.repo.root"))
        .resolve("spec/fixtures/providers/openai/responses")
    private fun fixture(name: String) = root.resolve(name).readBytes()
    private fun request() = ModelRequest(ModelRequestId("request-1"), ModelTargetId("gpt-5.4"),
        listOf(ModelCapability.TEXT, ModelCapability.TOOLS, ModelCapability.STREAMING),
        listOf(ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text("hello")))),
        listOf(ToolDescriptor("weather", "Lookup weather", "1",
            """{"type":"object","properties":{"city":{"type":"string"}},"required":["city"],"additionalProperties":false}""", true)),
        ModelOutputSettings(128u, TextMode.Plain, false), listOf("trace" to "fixture"))

    @Test fun `request matches shared official shape`() {
        val actual = assertIs<OpenAiResult.Success<*>>(OpenAiResponsesCodec.renderRequest(request(), true)).value
        assertEquals(Json.parseToJsonElement(fixture("request.json").decodeToString()), actual)
    }

    @Test fun `ordinary complete and truncated streams normalize`() {
        val ordinary = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseResponse(fixture("ordinary.json"))).value
        assertEquals(ModelStopReason.ToolUse, assertIs<InvokeOutcome.Completed>(ordinary).stopReason)
        val complete = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseSse(fixture("complete.sse"))).value
        assertEquals(ModelStopReason.EndTurn, assertIs<InvokeOutcome.Completed>(complete).stopReason)
        val truncated = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseSse(fixture("truncated.sse"))).value
        assertEquals("partial", assertIs<ModelItem.Text>(assertIs<InvokeOutcome.Interrupted>(truncated).partialItems.single()).text)
    }

    @Test fun `non increasing sequence fails closed`() {
        val malformed = fixture("complete.sse").decodeToString()
            .replaceFirst("\"sequence_number\":3", "\"sequence_number\":2").encodeToByteArray()
        assertEquals(OpenAiResult.Failure(OpenAiAdapterError.INVARIANT), OpenAiResponsesCodec.parseSse(malformed))
    }
}
