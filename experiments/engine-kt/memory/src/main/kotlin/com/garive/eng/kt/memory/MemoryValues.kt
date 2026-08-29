package com.garive.eng.kt.memory

import java.security.MessageDigest

/** Stable M0 validation, authority, version, or durability failure. */
public enum class MemoryErrorCode(public val wireName: String) {
    INVALID_MEMORY("invalid_memory"),
    NAMESPACE_DENIED("namespace_denied"),
    EVIDENCE_NOT_FOUND("evidence_not_found"),
    EVIDENCE_MISMATCH("evidence_mismatch"),
    REVISION_CONFLICT("revision_conflict"),
    RETENTION_REJECTED("retention_rejected"),
    SENSITIVITY_DENIED("sensitivity_denied"),
    LIMIT_EXCEEDED("limit_exceeded"),
    UNSUPPORTED("unsupported"),
    DURABILITY_FAILURE("durability_failure"),
    CORRUPT_MEMORY_STATE("corrupt_memory_state"),
    UNKNOWN_MEMORY_TYPE("unknown_memory_type"),
    AUTHORITY_RECEIPT_REQUIRED("authority_receipt_required"),
    SCOPE_POLICY_DENIED("scope_policy_denied"),
    INVALID_TRANSITION("invalid_transition"),
    DUPLICATE_OBSERVATION("duplicate_observation"),
    PROMOTION_RECEIPT_REQUIRED("promotion_receipt_required"),
    SELECTION_UNREPLAYABLE("selection_unreplayable"),
    ATTRIBUTION_UNSUPPORTED("attribution_unsupported"),
}

/** Typed M0 failure. */
public data class MemoryError(public val code: MemoryErrorCode)

/** Result of validating or reducing an M0 contract value. */
public sealed interface MemoryContractResult<out T> {
    public data class Success<T>(public val value: T) : MemoryContractResult<T>
    public data class Failure(public val error: MemoryError) : MemoryContractResult<Nothing>
}

/** Exact inline or Runtime-resolvable content with a SHA-256 binding. */
public class ContentBinding private constructor(
    public val digest: String,
    public val inlineUtf8: String?,
    public val reference: String?,
) {
    public companion object {
        /** Computes exact inline UTF-8 content binding. */
        public fun fromInline(inlineUtf8: String): ContentBinding =
            ContentBinding(sha256(inlineUtf8.encodeToByteArray()), inlineUtf8, null)

        /** Validates exact inline UTF-8 against a supplied digest. */
        public fun inline(digest: String, inlineUtf8: String): MemoryContractResult<ContentBinding> =
            if (!validDigest(digest) || sha256(inlineUtf8.encodeToByteArray()) != digest) {
                failure(MemoryErrorCode.INVALID_MEMORY)
            } else {
                MemoryContractResult.Success(ContentBinding(digest, inlineUtf8, null))
            }

        /** Validates an opaque Runtime-resolvable reference and asserted digest. */
        public fun referenced(digest: String, reference: String): MemoryContractResult<ContentBinding> =
            if (!validDigest(digest) || !validText(reference, MAX_REFERENCE_BYTES)) {
                failure(MemoryErrorCode.INVALID_MEMORY)
            } else {
                MemoryContractResult.Success(ContentBinding(digest, null, reference))
            }
    }

    public override fun equals(other: Any?): Boolean =
        other is ContentBinding && digest == other.digest && inlineUtf8 == other.inlineUtf8 && reference == other.reference
    public override fun hashCode(): Int = 31 * (31 * digest.hashCode() + inlineUtf8.hashCode()) + reference.hashCode()
}

/** Authorized scope of one memory record or query. */
public sealed interface MemoryScope {
    public data class Session(public val ownerId: String) : MemoryScope
    public data class AgentInstance(public val ownerId: String) : MemoryScope
    public data object Namespace : MemoryScope

    public companion object {
        /** Validates a Session scope. */
        public fun session(ownerId: String): MemoryContractResult<MemoryScope> =
            ownedScope(ownerId, ::Session)

        /** Validates an Agent-instance scope. */
        public fun agentInstance(ownerId: String): MemoryContractResult<MemoryScope> =
            ownedScope(ownerId, ::AgentInstance)
    }
}

/** Exact durable evidence binding verified by Runtime. */
@ConsistentCopyVisibility
public data class DurableFactReference private constructor(
    public val sessionId: String,
    public val position: ULong,
    public val factId: String,
    public val payloadDigest: String,
) : Comparable<DurableFactReference> {
    public override fun compareTo(other: DurableFactReference): Int = compareValuesBy(
        this, other, DurableFactReference::sessionId, DurableFactReference::position,
        DurableFactReference::factId, DurableFactReference::payloadDigest,
    )

    public companion object {
        /** Validates all four fixed-prefix evidence coordinates. */
        public fun create(
            sessionId: String,
            position: ULong,
            factId: String,
            payloadDigest: String,
        ): MemoryContractResult<DurableFactReference> =
            if (!validId(sessionId) || position == 0uL || !validId(factId) || !validDigest(payloadDigest)) {
                failure(MemoryErrorCode.INVALID_MEMORY)
            } else {
                MemoryContractResult.Success(DurableFactReference(sessionId, position, factId, payloadDigest))
            }
    }
}

/** Portable semantic class of a memory record. */
public enum class MemoryKind(public val wireName: String) {
    PREFERENCE("preference"),
    CONSTRAINT("constraint"),
    DECISION("decision"),
    LEARNED_FACT("learned_fact"),
    SUMMARY("summary"),
}

/** Lifecycle status of one immutable revision. */
public enum class MemoryStatus(public val wireName: String) {
    ACTIVE("active"),
    SUPERSEDED("superseded"),
    TOMBSTONED("tombstoned"),
}

/** Portable sensitivity class interpreted by Runtime authority. */
public enum class MemorySensitivity(public val wireName: String) {
    ORDINARY("ordinary"),
    RESTRICTED("restricted"),
}

internal const val MAX_ID_BYTES: Int = 128
internal const val MAX_REFERENCE_BYTES: Int = 512
internal const val MAX_BASIS_POINTS: Int = 10_000

internal fun failure(code: MemoryErrorCode): MemoryContractResult.Failure =
    MemoryContractResult.Failure(MemoryError(code))

internal fun validId(value: String): Boolean = validText(value, MAX_ID_BYTES)

internal fun validText(value: String, maxBytes: Int): Boolean =
    value.isNotEmpty() && value.encodeToByteArray().size <= maxBytes && value.trim() == value

internal fun validDigest(value: String): Boolean = value.matches(Regex("[0-9a-f]{64}"))

internal fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(value).joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }

internal fun <T : Comparable<T>> orderedUnique(values: List<T>): Boolean =
    values.zipWithNext().all { (left, right) -> left < right }

private fun ownedScope(
    ownerId: String,
    constructor: (String) -> MemoryScope,
): MemoryContractResult<MemoryScope> =
    if (validId(ownerId)) MemoryContractResult.Success(constructor(ownerId))
    else failure(MemoryErrorCode.INVALID_MEMORY)
