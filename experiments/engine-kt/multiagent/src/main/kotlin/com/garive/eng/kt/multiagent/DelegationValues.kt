package com.garive.eng.kt.multiagent

import java.security.MessageDigest

/** Stable MA0 validation, authority, budget, child, result, or durability failure. */
public enum class DelegationErrorCode(public val wireName: String) {
    INVALID_DELEGATION("invalid_delegation"), CHILD_NOT_FOUND("child_not_found"),
    CHILD_REVISION_MISMATCH("child_revision_mismatch"), AUTHORITY_DENIED("authority_denied"),
    BUDGET_EXHAUSTED("budget_exhausted"), BUDGET_OVERFLOW("budget_overflow"),
    DEPTH_EXCEEDED("depth_exceeded"), CONCURRENCY_EXCEEDED("concurrency_exceeded"),
    RESULT_SCHEMA_MISMATCH("result_schema_mismatch"), DELEGATION_CONFLICT("delegation_conflict"),
    CHILD_STATE_CORRUPT("child_state_corrupt"), DURABILITY_FAILURE("durability_failure"),
    CORRUPT_DELEGATION_STATE("corrupt_delegation_state"),
}

/** Typed portable MA0 result. */
public sealed interface DelegationContractResult<out T> {
    public data class Success<T>(public val value: T) : DelegationContractResult<T>
    public data class Failure(public val code: DelegationErrorCode) : DelegationContractResult<Nothing>
}

/** Exact inline content binding. */
@ConsistentCopyVisibility
public data class ContentBinding private constructor(
    public val digest: String,
    public val inlineUtf8: String?,
    public val reference: String?,
) {
    public companion object {
        /** Binds inline UTF-8 to its SHA-256 digest. */
        public fun fromInline(value: String): ContentBinding = ContentBinding(sha256(value.encodeToByteArray()), value, null)

        /** Validates an opaque Runtime-resolvable reference. */
        public fun referenced(digest: String, reference: String): DelegationContractResult<ContentBinding> =
            if (validDigest(digest) && validText(reference, MAX_REFERENCE_BYTES)) success(ContentBinding(digest, null, reference))
            else failure(DelegationErrorCode.INVALID_DELEGATION)
    }
}

/** Exact durable evidence coordinate admitted from a fixed Session prefix. */
@ConsistentCopyVisibility
public data class FactReference private constructor(
    public val sessionId: String,
    public val position: ULong,
    public val factId: String,
    public val payloadDigest: String,
) {
    public companion object {
        /** Validates a non-zero durable coordinate and digest. */
        public fun create(sessionId: String, position: ULong, factId: String, payloadDigest: String): DelegationContractResult<FactReference> =
            if (validId(sessionId) && position != 0uL && validId(factId) && validDigest(payloadDigest)) {
                success(FactReference(sessionId, position, factId, payloadDigest))
            } else failure(DelegationErrorCode.INVALID_DELEGATION)
    }
}

/** Complete finite delegation reservation and content bounds. */
public data class DelegationBudget(
    public val maxChildTurns: ULong, public val maxChildExecutions: ULong,
    public val maxIterations: ULong, public val maxInputTokens: ULong,
    public val maxOutputTokens: ULong, public val deadlineBudgetMs: ULong,
    public val maxDepth: ULong, public val maxObjectiveBytes: ULong,
    public val maxInputEvidence: ULong, public val maxResultSchemaBytes: ULong,
    public val maxResultBytes: ULong, public val maxResultEvidence: ULong,
) {
    /** V1 admits exactly one child Turn and requires every other bound. */
    public fun validate(): DelegationContractResult<Unit> =
        if (maxChildTurns == 1uL && listOf(
                maxChildExecutions, maxIterations, maxInputTokens, maxOutputTokens,
                deadlineBudgetMs, maxDepth, maxObjectiveBytes, maxInputEvidence,
                maxResultSchemaBytes, maxResultBytes, maxResultEvidence,
            ).all { it != 0uL }
        ) success(Unit) else failure(DelegationErrorCode.INVALID_DELEGATION)
}

internal const val MAX_REFERENCE_BYTES: Int = 512
internal fun validText(value: String, maxBytes: Int): Boolean =
    value.isNotEmpty() && value.trim() == value && value.encodeToByteArray().size <= maxBytes
internal fun validId(value: String): Boolean = validText(value, 128)
internal fun validDigest(value: String): Boolean = value.matches(Regex("[0-9a-f]{64}"))
internal fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }
internal fun <T> success(value: T): DelegationContractResult.Success<T> = DelegationContractResult.Success(value)
internal fun failure(code: DelegationErrorCode): DelegationContractResult.Failure = DelegationContractResult.Failure(code)
