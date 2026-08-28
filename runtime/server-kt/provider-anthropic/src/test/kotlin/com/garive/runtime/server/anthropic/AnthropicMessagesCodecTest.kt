package com.garive.runtime.server.anthropic

import com.garive.runtime.server.llm.*
import java.nio.file.Path
import java.time.Instant
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.*
import kotlin.time.Duration.Companion.seconds

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
        val http = assertIs<AnthropicResult.Success<HttpRequestDescriptor>>(
            AnthropicMessagesCodec.renderHttpRequest(request(), true)).value
        assertEquals("POST", http.method); assertEquals("/v1/messages", http.path)
        assertEquals("text/event-stream", http.headers.toMap()["accept"])
        assertEquals("2023-06-01", http.headers.toMap()["anthropic-version"])
        assertEquals(null, http.headers.toMap()["x-api-key"])
        assertEquals(actual, Json.parseToJsonElement(http.body.decodeToString()))
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
    @Test fun `thinking evidence matches ordinary and stream`() {
        val ordinary = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseResponse(fixture("thinking.json"))).value
        val streamed = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseSse(fixture("thinking.sse"))).value
        assertEquals(ordinary, streamed)
        assertEquals(4, assertIs<InvokeOutcome.Completed>(ordinary).items.size)
    }
    @Test fun `output limit and stream error are factual terminals`() {
        val body = """{"content":[{"type":"text","text":"partial"}],"stop_reason":"max_tokens","usage":{"input_tokens":2,"output_tokens":4}}"""
        val limited = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseResponse(body.encodeToByteArray())).value
        assertEquals(InterruptionKind.OUTPUT_LIMIT, assertIs<InvokeOutcome.Interrupted>(limited).reason)
        val streamError = assertIs<AnthropicResult.Success<InvokeOutcome>>(
            AnthropicMessagesCodec.parseSse(fixture("stream-error.sse"))).value
        assertEquals(UnavailableKind.MODEL_UNAVAILABLE, assertIs<InvokeOutcome.Unavailable>(streamError).reason)
    }
    @Test fun `shared HTTP errors and retry date normalize`() {
        val cases = Json.parseToJsonElement(fixture("errors.json").decodeToString())
            .jsonObject.getValue("cases").jsonArray
        cases.forEach { element -> val case = element.jsonObject
            val action = assertIs<AnthropicResult.Success<HttpErrorAction>>(
                AnthropicMessagesCodec.classifyHttpError(case.getValue("status").jsonPrimitive.int,
                    case["retry_after"]?.jsonPrimitive?.contentOrNull,
                    case.getValue("body").toString().encodeToByteArray(), true, Instant.EPOCH)).value
            assertEquals(case.getValue("expected").jsonPrimitive.content, render(action))
        }
        val retry = assertIs<AnthropicResult.Success<HttpErrorAction>>(
            AnthropicMessagesCodec.classifyHttpError(429, "Thu, 01 Jan 1970 00:00:03 GMT",
                """{"error":{"type":"rate_limit_error","message":"busy"}}""".encodeToByteArray(), false, Instant.EPOCH)).value
        assertEquals(HttpErrorAction.Retry(3.seconds), retry)
    }

    private fun render(action: HttpErrorAction): String = when (action) {
        is HttpErrorAction.Retry -> "retry:${action.retryAfter?.inWholeSeconds}"
        is HttpErrorAction.Terminal -> when (val outcome = action.outcome) {
            is InvokeOutcome.Rejected -> "rejected:${when (outcome.reason) {
                RejectionKind.CONTEXT_OVERFLOW -> "context-overflow"; RejectionKind.AUTHENTICATION -> "authentication"
                RejectionKind.CONTENT_POLICY -> "content-policy" }}"
            is InvokeOutcome.Unavailable -> when (outcome.reason) {
                UnavailableKind.RATE_LIMITED -> "unavailable:rate-limited:${outcome.retryAfter?.inWholeSeconds}"
                UnavailableKind.MODEL_UNAVAILABLE -> "unavailable:model-unavailable"
                UnavailableKind.CIRCUIT_OPEN -> "unavailable:circuit-open" }
            else -> "unexpected"
        }
    }
}
