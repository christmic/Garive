package com.garive.runtime.server.openai

import com.garive.runtime.server.llm.*
import java.nio.file.Path
import java.time.Instant
import kotlin.io.path.readBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.*
import kotlinx.coroutines.test.runTest
import kotlin.time.Duration.Companion.seconds

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
        val http = assertIs<OpenAiResult.Success<HttpRequestDescriptor>>(
            OpenAiResponsesCodec.renderHttpRequest(request(), true)).value
        assertEquals("POST", http.method); assertEquals("/v1/responses", http.path)
        assertEquals("text/event-stream", http.headers.toMap()["accept"])
        assertEquals(null, http.headers.toMap()["authorization"])
        assertEquals(actual, Json.parseToJsonElement(http.body.decodeToString()))
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

    @Test fun `incomplete stream preserves output limit partial`() {
        val outcome = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseSse(fixture("incomplete.sse"))).value
        val interrupted = assertIs<InvokeOutcome.Interrupted>(outcome)
        assertEquals(InterruptionKind.OUTPUT_LIMIT, interrupted.reason)
        assertEquals("partial", assertIs<ModelItem.Text>(interrupted.partialItems.single()).text)
    }

    @Test fun `composite stream and policy terminals preserve all admitted facts`() {
        val composite = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseSse(fixture("composite.sse"))).value
        val completed = assertIs<InvokeOutcome.Completed>(composite)
        assertEquals(4, completed.items.size)
        assertEquals(ModelStopReason.ToolUse, completed.stopReason)
        assertEquals("plan", assertIs<ReasoningContent.ModelVisible>(
            assertIs<ModelItem.Reasoning>(completed.items.first()).content).text)

        val filtered = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseResponse(fixture("content-filter.json"))).value
        assertEquals(RejectionKind.CONTENT_POLICY, assertIs<InvokeOutcome.Rejected>(filtered).reason)
        val refusal = assertIs<OpenAiResult.Success<InvokeOutcome>>(
            OpenAiResponsesCodec.parseResponse(fixture("refusal.json"))).value
        assertEquals(ModelStopReason.Refusal, assertIs<InvokeOutcome.Completed>(refusal).stopReason)
    }

    @Test fun `unknown semantic stream event fails closed`() {
        val unknown = """data: {"type":"response.created","sequence_number":0,"response":{"id":"resp","status":"in_progress"}}

data: {"type":"response.some_new_delta","sequence_number":1}

""".encodeToByteArray()
        assertEquals(OpenAiResult.Failure(OpenAiAdapterError.UNSUPPORTED_CAPABILITY),
            OpenAiResponsesCodec.parseSse(unknown))
        val malformed = fixture("composite.sse").decodeToString().replace(
            "\"part\":{\"type\":\"output_text\",\"text\":\"answer\",\"annotations\":[]}}",
            "\"part\":{\"type\":\"output_text\",\"text\":\"mismatch\",\"annotations\":[]}}"
        ).encodeToByteArray()
        assertEquals(OpenAiResult.Failure(OpenAiAdapterError.INVARIANT),
            OpenAiResponsesCodec.parseSse(malformed))
        val missingCreated = """data: {"type":"response.output_item.added","sequence_number":0,"output_index":0,"item":{"id":"msg","type":"message"}}

""".encodeToByteArray()
        assertEquals(OpenAiResult.Failure(OpenAiAdapterError.INVARIANT),
            OpenAiResponsesCodec.parseSse(missingCreated))
    }

    @Test fun `shared HTTP errors and retry date normalize`() {
        val cases = Json.parseToJsonElement(fixture("errors.json").decodeToString())
            .jsonObject.getValue("cases").jsonArray
        cases.forEach { element ->
            val case = element.jsonObject
            val action = assertIs<OpenAiResult.Success<HttpErrorAction>>(
                OpenAiResponsesCodec.classifyHttpError(
                    case.getValue("status").jsonPrimitive.int,
                    case["retry_after"]?.jsonPrimitive?.contentOrNull,
                    case.getValue("body").toString().encodeToByteArray(),
                    true,
                    Instant.EPOCH,
                )
            ).value
            assertEquals(case.getValue("expected").jsonPrimitive.content, render(action))
        }
        val retry = assertIs<OpenAiResult.Success<HttpErrorAction>>(
            OpenAiResponsesCodec.classifyHttpError(
                429,
                "Thu, 01 Jan 1970 00:00:03 GMT",
                """{"error":{"type":"rate_limit_error","code":"rate_limit_exceeded"}}""".encodeToByteArray(),
                false,
                Instant.EPOCH,
            )
        ).value
        assertEquals(HttpErrorAction.Retry(3.seconds), retry)
    }

    @Test fun `model port retries before ambiguity and returns one terminal`() = runTest {
        val transport = ScriptTransport(ArrayDeque(listOf(
            TransportResult.Success(HttpResponseDescriptor(429, "0",
                """{"error":{"type":"rate_limit_error","code":"rate_limit_exceeded"}}""".encodeToByteArray())),
            TransportResult.Success(HttpResponseDescriptor(200, null, fixture("complete.sse"))),
        )))
        val result = OpenAiModelPort(transport, 2).invoke(request(), ModelObserver { ObserverDecision.CONTINUE },
            ModelCancellation { false })
        assertIs<InvokeOutcome.Completed>(assertIs<ModelPortResult.Success>(result).outcome)
        assertEquals(listOf(kotlin.time.Duration.ZERO), transport.waits)
    }
    @Test fun `model port never retries ambiguous transport failure`() = runTest {
        val transport = ScriptTransport(ArrayDeque(listOf(
            TransportResult.Failure(TransportFailure.AMBIGUOUS),
            TransportResult.Success(HttpResponseDescriptor(200, null, fixture("complete.sse"))),
        )))
        val result = OpenAiModelPort(transport, 2).invoke(request(), ModelObserver { ObserverDecision.CONTINUE },
            ModelCancellation { false })
        assertIs<InvokeOutcome.Interrupted>(assertIs<ModelPortResult.Success>(result).outcome)
        assertEquals(1, transport.calls)
        assertEquals(emptyList(), transport.waits)
    }
    @Test fun `model port honors observer cancel with observed partial`() = runTest {
        val transport = ScriptTransport(
            ArrayDeque(
                listOf(TransportResult.Success(HttpResponseDescriptor(200, null, fixture("complete.sse")))),
            ),
        )
        val result = OpenAiModelPort(transport, 1).invoke(request(), ModelObserver { event ->
            if (event is ModelStreamEvent.OutputItemCompleted) ObserverDecision.CANCEL else ObserverDecision.CONTINUE
        }, ModelCancellation { false })
        val outcome = assertIs<InvokeOutcome.Interrupted>(assertIs<ModelPortResult.Success>(result).outcome)
        assertEquals(InterruptionKind.CANCELLED, outcome.reason)
        assertEquals(1, outcome.partialItems.size)
    }

    private class ScriptTransport(private val responses: ArrayDeque<TransportResult>) : OpenAiTransport {
        val waits = mutableListOf<kotlin.time.Duration>()
        var calls = 0
        override suspend fun execute(request: HttpRequestDescriptor, cancellation: ModelCancellation): TransportResult {
            calls += 1
            return responses.removeFirst()
        }
        override suspend fun wait(delay: kotlin.time.Duration) { waits += delay }
    }

    private fun render(action: HttpErrorAction): String = when (action) {
        is HttpErrorAction.Retry -> "retry:${action.retryAfter?.inWholeSeconds}"
        is HttpErrorAction.Terminal -> when (val outcome = action.outcome) {
            is InvokeOutcome.Rejected -> "rejected:${when (outcome.reason) {
                RejectionKind.CONTEXT_OVERFLOW -> "context-overflow"
                RejectionKind.AUTHENTICATION -> "authentication"
                RejectionKind.CONTENT_POLICY -> "content-policy"
            }}"
            is InvokeOutcome.Unavailable -> when (outcome.reason) {
                UnavailableKind.RATE_LIMITED -> "unavailable:rate-limited:${outcome.retryAfter?.inWholeSeconds}"
                UnavailableKind.MODEL_UNAVAILABLE -> "unavailable:model-unavailable"
                UnavailableKind.CIRCUIT_OPEN -> "unavailable:circuit-open"
            }
            else -> "unexpected"
        }
    }
}
