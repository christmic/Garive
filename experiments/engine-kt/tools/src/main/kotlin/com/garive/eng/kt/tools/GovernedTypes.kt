package com.garive.eng.kt.tools

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Structural construction result for C5 values. */
public sealed interface GovernedValueResult<out T> {
    /** Valid immutable value. */
    public data class Success<T>(public val value: T) : GovernedValueResult<T>
    /** Stable structural failure. */
    public data class Failure(public val label: String) : GovernedValueResult<Nothing>
}

private fun required(value: String, label: String): GovernedValueResult.Failure? =
    if (value.isEmpty()) GovernedValueResult.Failure(label) else null

/** Runtime-owned non-empty tool invocation identity. */
public class ToolInvocationId private constructor(public val value: String) {
    public companion object {
        /** Validates one identity. */
        public fun create(value: String): GovernedValueResult<ToolInvocationId> =
            required(value, "tool invocation") ?: GovernedValueResult.Success(ToolInvocationId(value))
    }
    public override fun equals(other: Any?): Boolean = other is ToolInvocationId && value == other.value
    public override fun hashCode(): Int = value.hashCode()
}

/** Runtime-owned non-empty interaction identity. */
public class InteractionId private constructor(public val value: String) {
    public companion object {
        /** Validates one identity. */
        public fun create(value: String): GovernedValueResult<InteractionId> =
            required(value, "interaction") ?: GovernedValueResult.Success(InteractionId(value))
    }
    public override fun equals(other: Any?): Boolean = other is InteractionId && value == other.value
    public override fun hashCode(): Int = value.hashCode()
}

/** Runtime-owned non-empty grant identity. */
public class GrantId private constructor(public val value: String) {
    public companion object {
        /** Validates one identity. */
        public fun create(value: String): GovernedValueResult<GrantId> =
            required(value, "grant") ?: GovernedValueResult.Success(GrantId(value))
    }
    public override fun equals(other: Any?): Boolean = other is GrantId && value == other.value
    public override fun hashCode(): Int = value.hashCode()
}

/** Runtime-owned non-empty receipt identity. */
public class ReceiptId private constructor(public val value: String) {
    public companion object {
        /** Validates one identity. */
        public fun create(value: String): GovernedValueResult<ReceiptId> =
            required(value, "receipt") ?: GovernedValueResult.Success(ReceiptId(value))
    }
    public override fun equals(other: Any?): Boolean = other is ReceiptId && value == other.value
    public override fun hashCode(): Int = value.hashCode()
}

/** Runtime-owned non-empty dispatch-attempt identity. */
public class DispatchAttemptId private constructor(public val value: String) {
    public companion object {
        /** Validates one identity. */
        public fun create(value: String): GovernedValueResult<DispatchAttemptId> =
            required(value, "dispatch attempt") ?: GovernedValueResult.Success(DispatchAttemptId(value))
    }
    public override fun equals(other: Any?): Boolean = other is DispatchAttemptId && value == other.value
    public override fun hashCode(): Int = value.hashCode()
}

/** Exact authority grant for one prepared invocation. */
public data class InvocationGrant(
    public val grantId: GrantId,
    public val invocationId: ToolInvocationId,
    public val preparedDigest: String,
    public val toolName: String,
    public val toolRevision: String,
    public val grantedRequirements: ExecutionRequirements,
    public val constraintsDigest: String,
    public val authorityRevision: String,
)

/** Human/product interaction kind. */
public enum class InteractionKind {
    /** Authority approval. */
    APPROVAL,
    /** Typed external input. */
    EXTERNAL_INPUT,
}

/** Exact interaction request bound to one invocation and digest. */
public data class InteractionRequest(
    public val interactionId: InteractionId,
    public val invocationId: ToolInvocationId,
    public val preparedDigest: String,
    public val kind: InteractionKind,
    public val prompt: JsonElement,
    public val responseSchema: JsonElement,
    public val expiryPolicy: String,
)

/** Typed continuation fact for one interaction. */
public sealed interface InteractionResolution {
    /** Schema-validated durable response. */
    public data class Resolved(
        public val interactionId: InteractionId,
        public val invocationId: ToolInvocationId,
        public val preparedDigest: String,
        public val response: JsonElement,
    ) : InteractionResolution
    /** Durable cancellation. */
    public data class Cancelled(
        public val interactionId: InteractionId,
        public val invocationId: ToolInvocationId,
        public val preparedDigest: String,
    ) : InteractionResolution
}

/** Trustworthy executor terminal classification. */
public enum class TerminalClassification {
    /** Proven success. */
    COMPLETED,
    /** Proven terminal failure. */
    FAILED,
}

/** Trustworthy executor receipt. */
public data class EffectReceipt(
    public val receiptId: ReceiptId,
    public val invocationId: ToolInvocationId,
    public val preparedDigest: String,
    public val grantId: GrantId,
    public val executorId: String,
    public val executorRevision: String,
    public val terminalClassification: TerminalClassification,
    public val resultDigest: String,
)

/** Fact delivered after Runtime durability boundaries complete. */
public sealed interface ExecutionFact {
    /** External dispatch boundary committed. */
    public data class Started(public val dispatchAttemptId: DispatchAttemptId) : ExecutionFact
    /** Successful receipt and bounded content. */
    public data class Completed(
        public val receipt: EffectReceipt?,
        public val content: JsonElement,
        public val truncated: Boolean,
    ) : ExecutionFact
    /** Trustworthy terminal failure. */
    public data class Failed(
        public val receipt: EffectReceipt?,
        public val code: String,
        public val details: String?,
        public val partial: JsonElement?,
    ) : ExecutionFact
    /** Started without trustworthy terminal evidence. */
    public data class Uncertain(public val evidence: String) : ExecutionFact
    /** Requirement cannot be enforced before Started. */
    public data class Unsupported(public val requirement: String) : ExecutionFact
}

/** Model-visible governed observation outcome. */
public sealed interface ObservationOutcome {
    /** Successful bounded tool content. */
    public data class Succeeded(public val content: JsonElement, public val truncated: Boolean) : ObservationOutcome
    /** Policy or interaction rejection. */
    public data class Rejected(public val code: String, public val details: String?) : ObservationOutcome
    /** Trustworthy terminal failure. */
    public data class Failed(public val code: String, public val details: String?, public val partial: JsonElement?) : ObservationOutcome
}

/** Exact model correlation and safe governed outcome. */
public data class GovernedObservation(
    public val invocationId: ToolInvocationId,
    public val preparedDigest: String,
    public val modelCallId: String,
    public val toolName: String,
    public val outcome: ObservationOutcome,
) {
    /** Stable neutral model-visible envelope. */
    public fun modelEnvelope(): JsonObject {
        val fields = linkedMapOf<String, JsonElement>()
        when (val value = outcome) {
            is ObservationOutcome.Succeeded -> fields.putAll(mapOf("status" to JsonPrimitive("succeeded"), "content" to value.content, "truncated" to JsonPrimitive(value.truncated)))
            is ObservationOutcome.Rejected -> { fields.putAll(mapOf("status" to JsonPrimitive("rejected"), "code" to JsonPrimitive(value.code))); value.details?.let { fields["details"] = JsonPrimitive(it) } }
            is ObservationOutcome.Failed -> { fields.putAll(mapOf("status" to JsonPrimitive("failed"), "code" to JsonPrimitive(value.code))); value.details?.let { fields["details"] = JsonPrimitive(it) }; value.partial?.let { fields["partial"] = it } }
        }
        return JsonObject(fields)
    }
}

/** Stable portable reducer failure. */
public enum class GovernedFailureCode {
    /** Grant binding or requirements mismatch. */
    GRANT_MISMATCH,
    /** Executor cannot enforce a requirement. */
    REQUIREMENT_UNSUPPORTED,
    /** Invocation transition conflicts. */
    INVOCATION_CONFLICT,
    /** Interaction binding conflicts. */
    INTERACTION_CONFLICT,
    /** Receipt or recovery state is corrupt. */
    CORRUPT_RECOVERY_STATE,
    /** Model correlation is invalid. */
    INVALID_MODEL_OUTPUT,
}

/** Portable lifecycle state. */
public enum class EffectState { PREPARED, DENIED, REPLACED, AWAITING_INTERACTION, AUTHORIZED, STARTED, COMPLETED, FAILED, UNCERTAIN }

/** Durable recovery position. */
public enum class RecoveryPosition { AUTHORIZED, STARTED_NO_RECEIPT, RECEIPT_NO_RESULT, TERMINAL }

/** Required deterministic recovery action. */
public enum class RecoveryDecision { REVALIDATE_GRANT, RETRY_SAME_INVOCATION, RECOVER_EXECUTOR_RECEIPT, RECONSTRUCT_FROM_RECEIPT, RETURN_TERMINAL, RECONCILE_OPERATOR }
