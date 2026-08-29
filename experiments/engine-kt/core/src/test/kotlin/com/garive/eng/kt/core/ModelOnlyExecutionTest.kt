package com.garive.eng.kt.core

import com.garive.eng.kt.llm.InterruptionKind
import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelCancellation
import com.garive.eng.kt.llm.ModelCapability
import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ModelObserver
import com.garive.eng.kt.llm.ModelOutputSettings
import com.garive.eng.kt.llm.ModelPort
import com.garive.eng.kt.llm.ModelPortFailure
import com.garive.eng.kt.llm.ModelPortResult
import com.garive.eng.kt.llm.ModelRequest
import com.garive.eng.kt.llm.ModelRole
import com.garive.eng.kt.llm.ModelStopReason
import com.garive.eng.kt.llm.ModelStreamEvent
import com.garive.eng.kt.llm.ModelTargetId
import com.garive.eng.kt.llm.ModelUsage
import com.garive.eng.kt.llm.ObserverDecision
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.TextMode
import com.garive.eng.kt.llm.TokenCount
import com.garive.eng.kt.llm.UnavailableKind
import com.garive.eng.kt.llm.UsageSource
import com.garive.eng.kt.tools.ExecutionCapability
import com.garive.eng.kt.tools.ExecutionRequirements
import com.garive.eng.kt.tools.GovernedToolResult
import com.garive.eng.kt.tools.PreparationError
import com.garive.eng.kt.tools.PreparationErrorCode
import com.garive.eng.kt.tools.PreparationRejectedFeedback
import com.garive.eng.kt.tools.PreparedToolCall
import com.garive.eng.kt.tools.ReplayClass
import com.garive.eng.kt.tools.ToolContractResult
import com.garive.eng.kt.tools.ToolDefinition
import com.garive.eng.kt.tools.ToolFeedback
import com.garive.eng.kt.tools.ToolIntent
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicInteger
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class ModelOnlyExecutionTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/agent/model-only-execution.json").readText()).jsonObject
    }

    @Test
    fun `Kotlin consumes every model-only scenario`() = runTest {
        val cases = document.getValue("cases").jsonArray
        assertEquals(25, cases.size)
        cases.forEach { element -> runCase(element.jsonObject) }
    }

    @Test
    fun `Kotlin governed loop commits tool feedback before next model request`() = runTest {
        val valid = runGoverned("completed-tool-read")
        assertEquals("completed", render(valid.report.outcome))
        assertEquals(listOf(1uL, 5uL), valid.positions)
        assertEquals(listOf(1, 1), valid.toolCounts)
        assertEquals(1, valid.effects.invocations)
        assertEquals(0, valid.effects.rejections)

        val rejected = runGoverned("completed-tool-missing")
        assertEquals("completed", render(rejected.report.outcome))
        assertEquals(listOf(1uL, 5uL), rejected.positions)
        assertEquals(0, rejected.effects.invocations)
        assertEquals(1, rejected.effects.rejections)
    }

    private suspend fun runCase(case: JsonObject) {
        val context = FakeContext(ArrayDeque(case.strings("contexts")))
        val model = FakeModel(ArrayDeque(case.strings("models")))
        val cancellation = FakeCancellation(case.optionalInt("cancel_after_checks"))
        val events = FakeEvents(case.optionalText("event_failure"))
        val ports = AgentExecutionPorts(context, model, events, cancellation) {
            if (case.optionalText("clock_failure") == "true") {
                Result.failure(IllegalStateException("clock"))
            } else {
                Result.success(case.optionalText("clock_tick")?.toULong() ?: 0u)
            }
        }
        val report = executeModelOnly(request(case), ports)
        val expected = case.obj("expected")
        assertEquals(expected.text("outcome"), render(report.outcome), case.text("name"))
        assertEquals(expected.uint("iterations"), report.completedIterations, case.text("name"))
        assertEquals(expected.int("context_calls"), context.calls, case.text("name"))
        assertEquals(expected.int("model_calls"), model.calls, case.text("name"))
        assertEquals(model.requestIds.size, model.requestIds.toSet().size, case.text("name"))
        document.getValue("usage_summary_cases").jsonArray
            .map { it.jsonObject }
            .find { it.text("execution_case") == case.text("name") }
            ?.obj("expected")
            ?.let { usage ->
                assertEquals(usage.text("input"), renderCount(report.usage.inputTokens))
                assertEquals(usage.text("output"), renderCount(report.usage.outputTokens))
                assertEquals(usage.text("estimated").toBoolean(), report.usage.estimated)
            }
        expected["targets"]?.jsonArray?.let { values ->
            assertEquals(values.map { it.jsonPrimitive.content }, model.targets, case.text("name"))
        }
    }

    private class FakeContext(private val scripts: ArrayDeque<String>) : ContextPort {
        var calls = 0
        val positions = mutableListOf<ULong>()
        override fun derive(request: ContextRequest, rebuildAttempt: UInt): ContextPortResult {
            calls += 1
            positions += request.throughPosition
            when (scripts.removeFirst()) {
                "failure" -> return ContextPortResult.Failure(PortFailure.CONTEXT)
                "required-budget" -> return ContextPortResult.RequiredFactsExceedBudget
            }
            val ref = FactRef(request.sessionId, request.throughPosition)
            return ContextPortResult.Success(
                ContextSurface(
                    ContextPurpose.INFERENCE,
                    1u,
                    request.throughPosition,
                    listOf(
                        ContextItem.Input(
                            ref,
                            ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text("hi"))),
                        ),
                    ),
                    listOf(ref),
                    emptyList(),
                    emptyList(),
                    1,
                    2,
                ),
            )
        }
    }

    private class FakeModel(private val scripts: ArrayDeque<String>) : ModelPort {
        var calls = 0
        val targets = mutableListOf<String>()
        val requestIds = mutableListOf<String>()
        val toolCounts = mutableListOf<Int>()
        override suspend fun invoke(
            request: ModelRequest,
            observer: ModelObserver,
            cancellation: ModelCancellation,
        ): ModelPortResult {
            calls += 1
            targets += request.targetId.value
            requestIds += request.requestId.value
            toolCounts += request.tools.size
            return when (val script = scripts.removeFirst()) {
                "completed-text-known" -> success(
                    InvokeOutcome.Completed(listOf(ModelItem.Text("done")), knownUsage(), ModelStopReason.EndTurn),
                )
                "completed-text-unknown" -> success(
                    InvokeOutcome.Completed(listOf(ModelItem.Text("done")), unknownUsage(), ModelStopReason.EndTurn),
                )
                "completed-tool-known" -> success(
                    InvokeOutcome.Completed(
                        listOf(ModelItem.ToolIntent("call", "tool", "{}")),
                        knownUsage(),
                        ModelStopReason.ToolUse,
                    ),
                )
                "completed-tool-read" -> success(
                    InvokeOutcome.Completed(
                        listOf(ModelItem.ToolIntent("call", "read_file", "{\"path\":\"a\"}")),
                        knownUsage(),
                        ModelStopReason.ToolUse,
                    ),
                )
                "completed-tool-missing" -> success(
                    InvokeOutcome.Completed(
                        listOf(ModelItem.ToolIntent("call", "missing", "{}")),
                        knownUsage(),
                        ModelStopReason.ToolUse,
                    ),
                )
                "context-overflow" -> success(InvokeOutcome.Rejected(RejectionKind.CONTEXT_OVERFLOW, "limit"))
                "output-limit" -> success(
                    InvokeOutcome.Interrupted(
                        InterruptionKind.OUTPUT_LIMIT,
                        listOf(ModelItem.Text("part")),
                        knownUsage(),
                    ),
                )
                "rate-limited" -> success(InvokeOutcome.Unavailable(UnavailableKind.RATE_LIMITED, null))
                "transport" -> success(
                    InvokeOutcome.Interrupted(InterruptionKind.TRANSPORT, emptyList(), knownUsage()),
                )
                "stream-event" -> {
                    assertEquals(
                        ObserverDecision.CANCEL,
                        observer.observe(ModelStreamEvent.TextDelta(0u, "x")),
                    )
                    success(InvokeOutcome.Interrupted(InterruptionKind.CANCELLED, emptyList(), knownUsage()))
                }
                "stream-cancel" -> {
                    assertEquals(
                        ObserverDecision.CANCEL,
                        observer.observe(ModelStreamEvent.TextDelta(0u, "x")),
                    )
                    success(InvokeOutcome.Interrupted(InterruptionKind.CANCELLED, emptyList(), knownUsage()))
                }
                "port-failure" -> ModelPortResult.Failure(ModelPortFailure.REQUIRED_PORT_FAILURE)
                else -> error("unknown model script $script")
            }
        }

        private fun success(outcome: InvokeOutcome) = ModelPortResult.Success(outcome)
    }

    private class FakeCancellation(private val cancelAfter: Int?) : ModelCancellation {
        private val checks = AtomicInteger()
        override fun isCancelled(): Boolean = cancelAfter?.let { checks.incrementAndGet() >= it } ?: false
    }

    private class FakeEvents(private val failure: String?) : EventSink {
        override fun emit(event: AgentEvent): PortFailure? =
            if (event.kind.code == failure) PortFailure.EVENT else null
    }

    private class FakeEffects : GovernedEffectPort {
        var invocations = 0
        var rejections = 0

        override suspend fun reject(
            intent: ToolIntent,
            error: PreparationError,
        ): Result<CommittedGovernedResult> {
            rejections += 1
            return Result.success(committed(intent, error))
        }

        override suspend fun invoke(prepared: PreparedToolCall): Result<CommittedGovernedResult> {
            invocations += 1
            return Result.success(
                committed(
                    ToolIntent(prepared.modelCallId, prepared.toolName, prepared.normalizedArguments),
                    PreparationError(PreparationErrorCode.TOOL_NOT_ADMITTED),
                ),
            )
        }

        private fun committed(intent: ToolIntent, error: PreparationError) = CommittedGovernedResult(
            GovernedToolResult.Observation(
                ToolFeedback.PreparationRejected(
                    PreparationRejectedFeedback(
                        intent.modelCallId,
                        intent.toolName,
                        error.code,
                        emptyList(),
                    ),
                ),
            ),
            5uL,
        )
    }

    private data class GovernedRun(
        val report: ExecutionReport,
        val positions: List<ULong>,
        val toolCounts: List<Int>,
        val effects: FakeEffects,
    )

    private suspend fun runGoverned(first: String): GovernedRun {
        val context = FakeContext(ArrayDeque(listOf("success", "success")))
        val model = FakeModel(ArrayDeque(listOf(first, "completed-text-known")))
        val effects = FakeEffects()
        val ports = AgentExecutionPorts(context, model, FakeEvents(null), FakeCancellation(null)) {
            Result.success(0uL)
        }
        val report = executeAgent(toolRequest(), AgentToolCapabilities(listOf(toolDefinition())), ports, effects)
        return GovernedRun(report, context.positions, model.toolCounts, effects)
    }

    private fun toolRequest(): AgentTurnRequest = AgentTurnRequest(
        SessionId.of("session"),
        TurnId.of("turn"),
        ExecutionId.of("execution"),
        AgentInstanceId.of("agent"),
        AgentDefinitionId.of("definition"),
        AgentDefinitionRevision.of("1"),
        AgentEntry.Start("hi"),
        AgentCursor(0u, 0uL),
        ContextRequest("session", "turn", ContextPurpose.INFERENCE, null, 1uL, 10, 100),
        listOf(ModelTargetId("primary")),
        listOf(ModelCapability.TOOLS),
        ModelOutputSettings(10uL, TextMode.Plain, false),
        ModelRecoveryPolicy(
            0u,
            OutputLimitAction.Fail,
            TerminalRecoveryAction.FAIL,
            TerminalRecoveryAction.FAIL,
            MissingUsagePolicy.Stop,
        ),
        ModelOnlyLimits(ExecutionLimits(3u), 100uL, null),
    )

    private fun toolDefinition(): ToolDefinition {
        val requirements = ExecutionRequirements.create(
            listOf(ExecutionCapability.FILESYSTEM_READ),
            1_000,
            1_024,
        ) as ToolContractResult.Success
        return (ToolDefinition.create(
            "read_file",
            "1",
            "Read one file.",
            Json.parseToJsonElement(
                """{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}""",
            ),
            requirements.value,
            ReplayClass.READ_ONLY,
        ) as ToolContractResult.Success).value
    }

    private fun request(case: JsonObject): AgentTurnRequest {
        val continuing = case.text("entry") == "continue"
        val lastPosition = case.ulong("last_position")
        return AgentTurnRequest(
            SessionId.of("session"),
            TurnId.of("turn"),
            ExecutionId.of(if (continuing) "exec-2" else "exec-1"),
            AgentInstanceId.of("agent"),
            AgentDefinitionId.of("definition"),
            AgentDefinitionRevision.of("1"),
            if (continuing) AgentEntry.Continue(ResumeInput.ResourceReady) else AgentEntry.Start("hi"),
            AgentCursor(case.uint("completed"), lastPosition),
            ContextRequest(
                "session", "turn", ContextPurpose.INFERENCE, null, maxOf(1uL, lastPosition), 10, 100,
            ),
            if (case.optionalText("unavailable") == "alternate") {
                listOf(ModelTargetId("primary"), ModelTargetId("secondary"))
            } else {
                listOf(ModelTargetId("primary"))
            },
            listOf(ModelCapability.TEXT, ModelCapability.STREAMING),
            ModelOutputSettings(10u, TextMode.Plain, false),
            ModelRecoveryPolicy(
                1u,
                when (case.optionalText("output_limit")) {
                    "retry:1" -> OutputLimitAction.Retry(1u)
                    "complete-partial" -> OutputLimitAction.CompletePartial
                    else -> OutputLimitAction.Suspend
                },
                TerminalRecoveryAction.SUSPEND,
                if (case.optionalText("unavailable") == "alternate") {
                    TerminalRecoveryAction.ALTERNATE_THEN_SUSPEND
                } else {
                    TerminalRecoveryAction.SUSPEND
                },
                if (case.text("missing_usage") == "estimate") {
                    MissingUsagePolicy.Estimate(3u, 2u)
                } else {
                    MissingUsagePolicy.Stop
                },
            ),
            ModelOnlyLimits(
                ExecutionLimits(case.uint("maximum")),
                case.ulong("max_tokens"),
                case.optionalText("deadline_tick")?.toULong(),
            ),
        )
    }

    private fun render(outcome: AgentOutcome): String = when (outcome) {
        is AgentOutcome.Completed -> "completed"
        is AgentOutcome.Suspended -> when (outcome.reason) {
            SuspensionReason.APPROVAL_REQUIRED -> "suspended:approval-required"
            SuspensionReason.EXTERNAL_INPUT_REQUIRED -> "suspended:external-input-required"
            SuspensionReason.OPERATOR_RECONCILIATION -> "suspended:operator-reconciliation"
            SuspensionReason.PARTIAL_OUTPUT -> "suspended:partial-output"
            SuspensionReason.RESOURCE_UNAVAILABLE -> "suspended:resource-unavailable"
        }
        is AgentOutcome.Stopped -> when (outcome.reason) {
            StopReason.ITERATION_LIMIT -> "stopped:iteration-limit"
            StopReason.TOKEN_LIMIT -> "stopped:token-limit"
            StopReason.CANCELLED -> "stopped:cancelled"
            StopReason.DEADLINE -> "stopped:deadline"
            else -> error("unexpected stop ${outcome.reason}")
        }
        is AgentOutcome.Failed -> when (outcome.reason) {
            AgentFailureReason.REQUIRED_CAPABILITY_UNAVAILABLE -> "failed:required-capability"
            AgentFailureReason.PORT_FAILURE -> "failed:port-failure"
            AgentFailureReason.INVALID_MODEL_OUTPUT -> "failed:invalid-model-output"
            else -> error("unexpected failure ${outcome.reason}")
        }
    }

    private fun renderCount(count: TokenCount): String = when (count) {
        is TokenCount.Known -> count.value.toString()
        TokenCount.Unknown -> "unknown"
    }

    companion object {
        private fun knownUsage() = ModelUsage(
            TokenCount.Known(1u), TokenCount.Known(1u), source = UsageSource.PROVIDER_REPORTED,
        )
        private fun unknownUsage() = ModelUsage(
            TokenCount.Unknown, TokenCount.Unknown, source = UsageSource.PROVIDER_REPORTED,
        )
    }

    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.optionalText(key: String) = get(key)?.jsonPrimitive?.content?.takeUnless { it == "null" }
    private fun JsonObject.optionalInt(key: String) = optionalText(key)?.toInt()
    private fun JsonObject.int(key: String) = text(key).toInt()
    private fun JsonObject.uint(key: String) = text(key).toUInt()
    private fun JsonObject.ulong(key: String) = text(key).toULong()
    private fun JsonObject.obj(key: String) = getValue(key).jsonObject
    private fun JsonObject.strings(key: String) = getValue(key).jsonArray.map { it.jsonPrimitive.content }
}
