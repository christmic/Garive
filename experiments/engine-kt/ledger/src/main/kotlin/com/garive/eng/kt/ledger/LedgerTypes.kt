package com.garive.eng.kt.ledger

import java.time.OffsetDateTime
import java.time.format.DateTimeParseException

/** Validated non-empty Session identity. */
@JvmInline public value class SessionId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a Session identity. */
        public fun of(value: String): SessionId = SessionId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated non-empty Turn identity. */
@JvmInline public value class TurnId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a Turn identity. */
        public fun of(value: String): TurnId = TurnId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated non-empty Execution identity. */
@JvmInline public value class ExecutionId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs an Execution identity. */
        public fun of(value: String): ExecutionId = ExecutionId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated Runtime-assigned idempotency identity. */
@JvmInline public value class FactId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a fact identity. */
        public fun of(value: String): FactId = FactId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated non-empty model-request identity. */
@JvmInline public value class ModelRequestId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a model-request identity. */
        public fun of(value: String): ModelRequestId = ModelRequestId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated non-empty tool-invocation identity. */
@JvmInline public value class ToolInvocationId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a tool-invocation identity. */
        public fun of(value: String): ToolInvocationId = ToolInvocationId(value.also { require(it.isNotEmpty()) })
    }
}

/** Stable non-empty semantic fact kind. */
@JvmInline public value class FactKind private constructor(public val value: String) : Comparable<FactKind> {
    public override fun compareTo(other: FactKind): Int = value.compareTo(other.value)
    public companion object {
        /** Validates and constructs a fact kind. */
        public fun of(value: String): FactKind = FactKind(value.also { require(it.isNotEmpty()) })
    }
}

/** Unpositioned fact supplied by Runtime as part of an atomic commit. */
public data class FactDraft(
    public val factId: FactId,
    public val turnId: TurnId?,
    public val executionId: ExecutionId?,
    public val modelRequestId: ModelRequestId?,
    public val toolInvocationId: ToolInvocationId?,
    public val kind: FactKind,
    public val schemaVersion: UInt,
    public val payload: CanonicalPayload,
    public val recordedAt: String,
) {
    /** Validates schema, RFC 3339 time, and canonical payload integrity. */
    public fun validate(): LedgerError? = when {
        schemaVersion == 0u || !recordedAt.isRfc3339() -> LedgerError.InvalidFact
        payload.verify() != null -> LedgerError.DigestMismatch
        else -> null
    }

    /** Compares idempotency-bound fields while excluding observation time. */
    public fun sameSemantics(other: FactDraft): Boolean =
        factId == other.factId && turnId == other.turnId && executionId == other.executionId &&
            modelRequestId == other.modelRequestId && toolInvocationId == other.toolInvocationId &&
            kind == other.kind && schemaVersion == other.schemaVersion && payload == other.payload
}

/** Immutable fact after assignment to a Session-local durable position. */
public data class DurableFact(
    public val factId: FactId,
    public val sessionId: SessionId,
    public val position: ULong,
    public val turnId: TurnId?,
    public val executionId: ExecutionId?,
    public val modelRequestId: ModelRequestId?,
    public val toolInvocationId: ToolInvocationId?,
    public val kind: FactKind,
    public val schemaVersion: UInt,
    public val payload: CanonicalPayload,
    public val recordedAt: String,
) {
    /** Verifies position, schema, timestamp, and payload integrity. */
    public fun verify(): LedgerError? = when {
        position == 0uL || schemaVersion == 0u || !recordedAt.isRfc3339() -> LedgerError.Corruption
        payload.verify() == CanonicalPayloadError.DIGEST_MISMATCH -> LedgerError.DigestMismatch
        else -> null
    }
}

/** Whether commit appended facts or replayed an identical prior batch. */
public enum class CommitDisposition { COMMITTED, REPLAYED }

/** Durable coordinates returned by commit/replay. */
public data class CommitResult(
    public val disposition: CommitDisposition,
    public val sessionVersion: ULong,
    public val positions: List<ULong>,
)

/** Typed failure from Ledger validation, transitions, concurrency, or integrity. */
public sealed class LedgerError protected constructor(public val code: String) {
    public data object EmptyBatch : LedgerError("empty-batch")
    public data object ConcurrentModification : LedgerError("concurrent-modification")
    public data object IdempotencyCollision : LedgerError("idempotency-collision")
    public data object IncompleteReplay : LedgerError("incomplete-replay")
    public data object InvalidFact : LedgerError("invalid-fact")
    public data object InvalidTransition : LedgerError("invalid-transition")
    public data object MissingReference : LedgerError("missing-reference")
    public data object PositionOverflow : LedgerError("position-overflow")
    public data object InvalidReadRange : LedgerError("invalid-read-range")
    public data object DigestMismatch : LedgerError("digest-mismatch")
    public data object Corruption : LedgerError("corruption")
}

/** Success/failure envelope for portable Ledger operations. */
public sealed interface LedgerResult<out T> {
    public data class Success<T>(public val value: T) : LedgerResult<T>
    public data class Failure(public val error: LedgerError) : LedgerResult<Nothing>
}

private fun String.isRfc3339() = try {
    OffsetDateTime.parse(this)
    true
} catch (_: DateTimeParseException) {
    false
}
