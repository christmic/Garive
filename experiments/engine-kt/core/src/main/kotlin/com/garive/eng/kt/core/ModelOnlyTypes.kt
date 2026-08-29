package com.garive.eng.kt.core

import com.garive.eng.kt.llm.ModelCancellation
import com.garive.eng.kt.llm.ModelCapability
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ModelOutputSettings
import com.garive.eng.kt.llm.ModelPort
import com.garive.eng.kt.llm.ModelStreamEvent
import com.garive.eng.kt.llm.ModelTargetId
import com.garive.eng.kt.llm.TokenCount
import com.garive.eng.kt.skill.ActivatedSkill
import java.security.MessageDigest
import java.time.Instant

/** Typed durable evidence used to continue an open Turn. */
public sealed interface ResumeInput {
    public data class ExternalInput(public val value: String) : ResumeInput
    public data class Reconciliation(public val value: String) : ResumeInput
    public data object ResourceReady : ResumeInput
}

/** Runtime-selected entry mode for one disposable Execution. */
public sealed interface AgentEntry {
    public data class Start(public val trustedInput: String) : AgentEntry
    public data class Continue(public val resumeInput: ResumeInput) : AgentEntry
}

/** Durable progress reconstructed before Core begins. */
public data class AgentCursor(
    public val completedIterations: UInt,
    public val lastDurablePosition: ULong,
)

/** Policy for provider results without usable token counts. */
public sealed interface MissingUsagePolicy {
    public data object Stop : MissingUsagePolicy
    public data class Estimate(public val inputTokens: ULong, public val outputTokens: ULong) : MissingUsagePolicy
}

/** Terminal policy action after a recoverable dependency failure. */
public enum class TerminalRecoveryAction { SUSPEND, STOP, FAIL, ALTERNATE_THEN_SUSPEND }

/** Bounded policy action after output-limit interruption. */
public sealed interface OutputLimitAction {
    public data object CompletePartial : OutputLimitAction
    public data class Retry(public val maxRetries: UInt) : OutputLimitAction
    public data object Suspend : OutputLimitAction
    public data object Stop : OutputLimitAction
    public data object Fail : OutputLimitAction
}

/** Immutable bounded recovery decisions for model-only execution. */
public data class ModelRecoveryPolicy(
    public val maxContextRebuilds: UInt,
    public val outputLimit: OutputLimitAction,
    public val transport: TerminalRecoveryAction,
    public val unavailable: TerminalRecoveryAction,
    public val missingUsage: MissingUsagePolicy,
)

/** Iteration, token, and logical-deadline bounds. */
public data class ModelOnlyLimits(
    public val execution: ExecutionLimits,
    public val maxTotalTokens: ULong?,
    public val deadlineTick: ULong?,
)

/** Exact durable evidence attribution attached to model-visible Memory data. */
public data class MemoryEvidenceAttribution(
    public val sessionId: String,
    public val position: ULong,
    public val factId: String,
    public val payloadDigest: String,
)

/** Runtime-verified inline Memory supplied as subordinate attributed data. */
public data class AttributedMemory(
    public val recordId: String,
    public val revisionId: String,
    public val contentDigest: String,
    public val contentUtf8: String,
    public val evidence: List<MemoryEvidenceAttribution>,
) {
    internal fun valid(): Boolean = recordId.isNotEmpty() && revisionId.isNotEmpty() && contentUtf8.isNotEmpty() &&
        contentDigest == sha256(contentUtf8.encodeToByteArray()) && evidence.isNotEmpty() &&
        evidence.all { it.valid() }
}

private fun MemoryEvidenceAttribution.valid(): Boolean =
    sessionId.isNotEmpty() && position != 0uL && factId.isNotEmpty() && payloadDigest.matches(Regex("[0-9a-f]{64}"))

/** Exact sanitized citation attached to model-visible Knowledge evidence. */
public data class KnowledgeCitationAttribution(
    public val locatorKind: String,
    public val locator: String,
    public val title: String?,
    public val canonicalUri: String?,
    public val contentDigest: String,
)

/** Runtime-verified Knowledge evidence supplied as subordinate attributed data. */
public data class AttributedKnowledge(
    public val sourceId: String,
    public val sourceRevision: String,
    public val evidenceId: String,
    public val sourceSnapshotDigest: String?,
    public val contentDigest: String,
    public val contentUtf8: String,
    public val contentByteLength: ULong,
    public val citation: KnowledgeCitationAttribution,
    public val retrievedAtUtc: String,
    public val freshness: String,
    public val trustClass: String,
    public val rankBasisPoints: UShort,
) {
    internal fun valid(): Boolean = sourceId.isNotEmpty() && sourceRevision.isNotEmpty() && evidenceId.isNotEmpty() &&
        (sourceSnapshotDigest == null || sourceSnapshotDigest.isDigest()) && contentUtf8.isNotEmpty() &&
        contentDigest == sha256(contentUtf8.encodeToByteArray()) &&
        contentByteLength == contentUtf8.encodeToByteArray().size.toULong() && citation.valid(contentDigest) &&
        runCatching { Instant.parse(retrievedAtUtc).toString() == retrievedAtUtc }.getOrDefault(false) &&
        freshness in setOf("fresh", "cached", "stale") &&
        trustClass in setOf("curated", "first_party", "third_party", "untrusted") && rankBasisPoints <= 10_000u
}

private fun KnowledgeCitationAttribution.valid(expectedDigest: String): Boolean =
    locatorKind in setOf("uri_fragment", "document_offset", "record_key", "opaque_locator") &&
        locator.isNotEmpty() && (title == null || title.isNotEmpty()) &&
        (canonicalUri == null || canonicalUri.isNotEmpty()) && contentDigest == expectedDigest

private fun String.isDigest(): Boolean = matches(Regex("[0-9a-f]{64}"))

/** Complete immutable input for one model-only kernel Execution. */
public data class AgentTurnRequest(
    public val sessionId: SessionId,
    public val turnId: TurnId,
    public val executionId: ExecutionId,
    public val agentInstanceId: AgentInstanceId,
    public val definitionId: AgentDefinitionId,
    public val definitionRevision: AgentDefinitionRevision,
    public val entry: AgentEntry,
    public val cursor: AgentCursor,
    public val contextRequest: ContextRequest,
    public val modelTargets: List<ModelTargetId>,
    public val requiredCapabilities: List<ModelCapability>,
    public val modelOutput: ModelOutputSettings,
    public val recoveryPolicy: ModelRecoveryPolicy,
    public val limits: ModelOnlyLimits,
    public val activatedSkills: List<ActivatedSkill> = emptyList(),
    public val capabilityContextCandidates: List<ContextCandidate> = emptyList(),
    public val attributedMemory: List<AttributedMemory> = emptyList(),
    public val attributedKnowledge: List<AttributedKnowledge> = emptyList(),
) {
    /** Validates cross-field invariants before any port is invoked. */
    public fun validate(): AgentRequestError? {
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
        if (attributedMemory.any { !it.valid() }) return AgentRequestError.INVALID_MEMORY_CONTEXT
        if (attributedKnowledge.any { !it.valid() }) return AgentRequestError.INVALID_KNOWLEDGE_CONTEXT
        return null
    }
}

/** Stable validation failure in [AgentTurnRequest]. */
public enum class AgentRequestError {
    ENTRY_CURSOR_MISMATCH,
    SESSION_MISMATCH,
    MISSING_MODEL_TARGET,
    INVALID_MODEL_TARGET,
    INVALID_TOKEN_LIMIT,
    INVALID_MEMORY_CONTEXT,
    INVALID_KNOWLEDGE_CONTEXT,
}

private fun sha256(value: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }

/** Checked usage accumulated across all model attempts. */
public data class UsageSummary(
    public val inputTokens: TokenCount,
    public val outputTokens: TokenCount,
    public val estimated: Boolean,
)
/** Requirement that keeps the durable Turn resumable. */
public enum class SuspensionReason {
    APPROVAL_REQUIRED,
    EXTERNAL_INPUT_REQUIRED,
    OPERATOR_RECONCILIATION,
    PARTIAL_OUTPUT,
    RESOURCE_UNAVAILABLE,
    DELEGATION_PENDING,
}
/** Expected policy boundary that stops a Turn. */
public enum class StopReason { ITERATION_LIMIT, TOKEN_LIMIT, DEADLINE, CANCELLED, RESOURCE_UNAVAILABLE }
/** Non-success Execution failure classification. */
public enum class AgentFailureReason {
    INVALID_INPUT,
    INVALID_MODEL_OUTPUT,
    REQUIRED_CAPABILITY_UNAVAILABLE,
    PORT_FAILURE,
    INVARIANT_VIOLATION,
}

/** Exactly one terminal proposal returned to Runtime. */
public sealed interface AgentOutcome {
    public data class Completed(
        public val responseItems: List<ModelItem>,
        public val usage: UsageSummary,
    ) : AgentOutcome
    public data class Suspended(
        public val reason: SuspensionReason,
        public val partialItems: List<ModelItem>,
        public val lastDurablePosition: ULong,
        public val governedBinding: GovernedSuspensionBinding?,
    ) : AgentOutcome
    public data class Stopped(public val reason: StopReason) : AgentOutcome
    public data class Failed(public val reason: AgentFailureReason) : AgentOutcome
}

/** Ordered semantic progress emitted for Runtime persistence/publication. */
public sealed interface AgentEventKind {
    public val code: String
    public data object ExecutionStarted : AgentEventKind { public override val code: String = "execution-started" }
    public data class IterationStarted(public val iteration: UInt) : AgentEventKind {
        public override val code: String = "iteration-started"
    }
    public data class ContextDerived(public val itemCount: Int, public val utf8Bytes: Int) : AgentEventKind {
        public override val code: String = "context-derived"
    }
    public data class ModelRequestPrepared(public val requestId: String, public val targetId: String) : AgentEventKind {
        public override val code: String = "model-request-prepared"
    }
    public data class ModelStream(public val event: ModelStreamEvent) : AgentEventKind {
        public override val code: String = "model-stream"
    }
    public data object OutcomeProposed : AgentEventKind { public override val code: String = "outcome-proposed" }
}

/** Semantic event with identities required for durable attribution. */
public data class AgentEvent(
    public val sessionId: SessionId,
    public val turnId: TurnId,
    public val executionId: ExecutionId,
    public val kind: AgentEventKind,
)

/** Sanitized class of a frozen execution-port failure. */
public enum class PortFailure { CONTEXT, EVENT, CLOCK, TOOL }
/** Context port result containing the frozen Runtime read. */
public sealed interface ContextPortResult {
    public data class Success(public val candidates: List<ContextCandidate>) : ContextPortResult
    public data class Failure(public val failure: PortFailure) : ContextPortResult
}
/** Frozen purpose-specific candidate read port. */
public fun interface ContextPort {
    public fun readCandidates(request: ContextRequest, rebuildAttempt: UInt): ContextPortResult
}
/** Sink for ordered semantic progress; emission is not proof of persistence. */
public fun interface EventSink {
    public fun emit(event: AgentEvent): PortFailure?
}
/** Runtime-owned logical clock used for deterministic deadlines. */
public fun interface ClockPort {
    public fun nowTick(): Result<ULong>
}

/** Frozen external capabilities for one model-only Execution. */
public data class AgentExecutionPorts(
    public val context: ContextPort,
    public val model: ModelPort,
    public val events: EventSink,
    public val cancellation: ModelCancellation,
    public val clock: ClockPort,
)

/** Terminal proposal plus the durable iteration cursor Runtime must commit. */
public data class ExecutionReport(
    public val outcome: AgentOutcome,
    public val completedIterations: UInt,
    public val usage: UsageSummary,
)
