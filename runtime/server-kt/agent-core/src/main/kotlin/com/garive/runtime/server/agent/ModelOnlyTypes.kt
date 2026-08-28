package com.garive.runtime.server.agent

import com.garive.runtime.server.llm.ModelCancellation
import com.garive.runtime.server.llm.ModelCapability
import com.garive.runtime.server.llm.ModelItem
import com.garive.runtime.server.llm.ModelOutputSettings
import com.garive.runtime.server.llm.ModelPort
import com.garive.runtime.server.llm.ModelStreamEvent
import com.garive.runtime.server.llm.ModelTargetId

sealed interface ResumeInput {
    data class ExternalInput(val value: String) : ResumeInput
    data class Reconciliation(val value: String) : ResumeInput
    data object ResourceReady : ResumeInput
}

sealed interface AgentEntry {
    data class Start(val trustedInput: String) : AgentEntry
    data class Continue(val resumeInput: ResumeInput) : AgentEntry
}

data class AgentCursor(val completedIterations: UInt, val lastDurablePosition: ULong)

sealed interface MissingUsagePolicy {
    data object Stop : MissingUsagePolicy
    data class Estimate(val inputTokens: ULong, val outputTokens: ULong) : MissingUsagePolicy
}

enum class TerminalRecoveryAction { SUSPEND, STOP, FAIL, ALTERNATE_THEN_SUSPEND }

sealed interface OutputLimitAction {
    data object CompletePartial : OutputLimitAction
    data class Retry(val maxRetries: UInt) : OutputLimitAction
    data object Suspend : OutputLimitAction
    data object Stop : OutputLimitAction
    data object Fail : OutputLimitAction
}

data class ModelRecoveryPolicy(
    val maxContextRebuilds: UInt,
    val outputLimit: OutputLimitAction,
    val transport: TerminalRecoveryAction,
    val unavailable: TerminalRecoveryAction,
    val missingUsage: MissingUsagePolicy,
)

data class ModelOnlyLimits(
    val execution: ExecutionLimits,
    val maxTotalTokens: ULong?,
    val deadlineTick: ULong?,
)

data class AgentTurnRequest(
    val sessionId: SessionId,
    val turnId: TurnId,
    val executionId: ExecutionId,
    val agentInstanceId: AgentInstanceId,
    val definitionId: AgentDefinitionId,
    val definitionRevision: AgentDefinitionRevision,
    val entry: AgentEntry,
    val cursor: AgentCursor,
    val contextRequest: ContextRequest,
    val modelTargets: List<ModelTargetId>,
    val requiredCapabilities: List<ModelCapability>,
    val modelOutput: ModelOutputSettings,
    val recoveryPolicy: ModelRecoveryPolicy,
    val limits: ModelOnlyLimits,
) {
    fun validate(): AgentRequestError? {
        if (entry is AgentEntry.Start && (cursor.completedIterations != 0u || cursor.lastDurablePosition != 0uL)) {
            return AgentRequestError.ENTRY_CURSOR_MISMATCH
        }
        if (entry is AgentEntry.Continue && cursor.lastDurablePosition == 0uL) {
            return AgentRequestError.ENTRY_CURSOR_MISMATCH
        }
        if (contextRequest.sessionId != sessionId.value) return AgentRequestError.SESSION_MISMATCH
        if (modelTargets.isEmpty()) return AgentRequestError.MISSING_MODEL_TARGET
        if (modelTargets.any { it.value.isEmpty() }) return AgentRequestError.INVALID_MODEL_TARGET
        if (limits.maxTotalTokens == 0uL) return AgentRequestError.INVALID_TOKEN_LIMIT
        return null
    }
}

enum class AgentRequestError {
    ENTRY_CURSOR_MISMATCH,
    SESSION_MISMATCH,
    MISSING_MODEL_TARGET,
    INVALID_MODEL_TARGET,
    INVALID_TOKEN_LIMIT,
}

data class UsageSummary(val inputTokens: ULong, val outputTokens: ULong, val estimated: Boolean)
enum class SuspensionReason { PARTIAL_OUTPUT, RESOURCE_UNAVAILABLE }
enum class StopReason { ITERATION_LIMIT, TOKEN_LIMIT, DEADLINE, CANCELLED, RESOURCE_UNAVAILABLE }
enum class AgentFailureReason {
    INVALID_INPUT,
    INVALID_MODEL_OUTPUT,
    REQUIRED_CAPABILITY_UNAVAILABLE,
    PORT_FAILURE,
    INVARIANT_VIOLATION,
}

sealed interface AgentOutcome {
    data class Completed(val responseItems: List<ModelItem>, val usage: UsageSummary) : AgentOutcome
    data class Suspended(
        val reason: SuspensionReason,
        val partialItems: List<ModelItem>,
        val lastDurablePosition: ULong,
    ) : AgentOutcome
    data class Stopped(val reason: StopReason) : AgentOutcome
    data class Failed(val reason: AgentFailureReason) : AgentOutcome
}

sealed interface AgentEventKind {
    val code: String
    data object ExecutionStarted : AgentEventKind { override val code = "execution-started" }
    data class IterationStarted(val iteration: UInt) : AgentEventKind { override val code = "iteration-started" }
    data class ContextDerived(val itemCount: Int, val utf8Bytes: Int) : AgentEventKind { override val code = "context-derived" }
    data class ModelRequestPrepared(val requestId: String, val targetId: String) : AgentEventKind { override val code = "model-request-prepared" }
    data class ModelStream(val event: ModelStreamEvent) : AgentEventKind { override val code = "model-stream" }
    data object OutcomeProposed : AgentEventKind { override val code = "outcome-proposed" }
}

data class AgentEvent(
    val sessionId: SessionId,
    val turnId: TurnId,
    val executionId: ExecutionId,
    val kind: AgentEventKind,
)

enum class PortFailure { CONTEXT, EVENT, CLOCK }
sealed interface ContextPortResult {
    data class Success(val surface: ContextSurface) : ContextPortResult
    data object RequiredFactsExceedBudget : ContextPortResult
    data class Failure(val failure: PortFailure) : ContextPortResult
}
fun interface ContextPort { fun derive(request: ContextRequest, rebuildAttempt: UInt): ContextPortResult }
fun interface EventSink { fun emit(event: AgentEvent): PortFailure? }
fun interface ClockPort { fun nowTick(): Result<ULong> }

data class AgentExecutionPorts(
    val context: ContextPort,
    val model: ModelPort,
    val events: EventSink,
    val cancellation: ModelCancellation,
    val clock: ClockPort,
)

data class ExecutionReport(val outcome: AgentOutcome, val completedIterations: UInt)
