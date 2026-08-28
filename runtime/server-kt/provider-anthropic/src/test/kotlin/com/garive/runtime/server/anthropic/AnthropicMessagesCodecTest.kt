package com.garive.runtime.server.anthropic

import com.garive.runtime.server.llm.*
import java.nio.file.Path
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json

class AnthropicMessagesCodecTest {
    private val root = Path.of(System.getProperty("garive.repo.root"))
        .resolve("spec/fixtures/providers/anthropic/messages")
    private fun fixture(name: String) = root.resolve(name).readBytes()
    private fun request() = ModelRequest(ModelRequestId("request-1"), ModelTargetId("claude-sonnet-4-5"),
        listOf(ModelCapability.TEXT, ModelCapability.TOOLS, ModelCapability.STREAMING),
        listOf(ModelInputItem.Message(ModelRole.SYSTEM, listOf(ModelInputContent.Text("be concise"))),
            ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text("hello")))),
        listOf(ToolDescriptor("weather", "Lookup weather", "1",
            """{"type":"object","properties":{"city":{"type":"string"}},"required":["city"],"additionalProperties":false}""", false)),
        ModelOutputSettings(128u, TextMode.Plain, false), listOf("user_id" to "fixture"))

    @Test fun `request matches shared official shape`() {
        val actual = assertIs<AnthropicResult.Success<*>>(AnthropicMessagesCodec.renderRequest(request(), true)).value
        assertEquals(Json.parseToJsonElement(fixture("request.json").decodeToString()), actual)
    }
    @Test fun `ordinary complete and truncated streams normalize`() {
        val ordinary = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseResponse(fixture("ordinary.json"))).value
        assertEquals(ModelStopReason.ToolUse, assertIs<InvokeOutcome.Completed>(ordinary).stopReason)
        val complete = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseSse(fixture("complete.sse"))).value
        assertEquals(2, assertIs<InvokeOutcome.Completed>(complete).items.size)
        val truncated = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseSse(fixture("truncated.sse"))).value
        assertEquals("partial", assertIs<ModelItem.Text>(assertIs<InvokeOutcome.Interrupted>(truncated).partialItems.single()).text)
    }
    @Test fun `unclosed block fails terminal`() {
        val malformed = fixture("complete.sse").decodeToString().replace(
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n", "").encodeToByteArray()
        assertEquals(AnthropicResult.Failure(AnthropicAdapterError.INVARIANT), AnthropicMessagesCodec.parseSse(malformed))
    }
}
