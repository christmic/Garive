package com.garive.runtime.server.ledger

import java.time.OffsetDateTime
import java.time.format.DateTimeParseException

@JvmInline value class SessionId private constructor(val value: String) {
    companion object { fun of(value: String) = SessionId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class TurnId private constructor(val value: String) {
    companion object { fun of(value: String) = TurnId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class ExecutionId private constructor(val value: String) {
    companion object { fun of(value: String) = ExecutionId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class FactId private constructor(val value: String) {
    companion object { fun of(value: String) = FactId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class ModelRequestId private constructor(val value: String) {
    companion object { fun of(value: String) = ModelRequestId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class ToolInvocationId private constructor(val value: String) {
    companion object { fun of(value: String) = ToolInvocationId(value.also { require(it.isNotEmpty()) }) }
}

@JvmInline value class FactKind private constructor(val value: String) : Comparable<FactKind> {
    override fun compareTo(other: FactKind) = value.compareTo(other.value)
    companion object { fun of(value: String) = FactKind(value.also { require(it.isNotEmpty()) }) }
}

data class FactDraft(
    val factId: FactId,
    val turnId: TurnId?,
    val executionId: ExecutionId?,
    val modelRequestId: ModelRequestId?,
    val toolInvocationId: ToolInvocationId?,
    val kind: FactKind,
    val schemaVersion: UInt,
    val payload: CanonicalPayload,
    val recordedAt: String,
) {
    fun validate(): LedgerError? = when {
        schemaVersion == 0u || !recordedAt.isRfc3339() -> LedgerError.InvalidFact
        payload.verify() != null -> LedgerError.DigestMismatch
        else -> null
    }

    fun sameSemantics(other: FactDraft) =
        factId == other.factId && turnId == other.turnId && executionId == other.executionId &&
            modelRequestId == other.modelRequestId && toolInvocationId == other.toolInvocationId &&
            kind == other.kind && schemaVersion == other.schemaVersion && payload == other.payload
}

data class DurableFact(
    val factId: FactId,
    val sessionId: SessionId,
    val position: ULong,
    val turnId: TurnId?,
    val executionId: ExecutionId?,
    val modelRequestId: ModelRequestId?,
    val toolInvocationId: ToolInvocationId?,
    val kind: FactKind,
    val schemaVersion: UInt,
    val payload: CanonicalPayload,
    val recordedAt: String,
) {
    fun verify(): LedgerError? = when {
        position == 0uL || schemaVersion == 0u || !recordedAt.isRfc3339() -> LedgerError.Corruption
        payload.verify() == CanonicalPayloadError.DIGEST_MISMATCH -> LedgerError.DigestMismatch
        else -> null
    }
}

enum class CommitDisposition { COMMITTED, REPLAYED }
data class CommitResult(
    val disposition: CommitDisposition,
    val sessionVersion: ULong,
    val positions: List<ULong>,
)

sealed class LedgerError(val code: String) {
    data object EmptyBatch : LedgerError("empty-batch")
    data object ConcurrentModification : LedgerError("concurrent-modification")
    data object IdempotencyCollision : LedgerError("idempotency-collision")
    data object IncompleteReplay : LedgerError("incomplete-replay")
    data object InvalidFact : LedgerError("invalid-fact")
    data object InvalidTransition : LedgerError("invalid-transition")
    data object MissingReference : LedgerError("missing-reference")
    data object PositionOverflow : LedgerError("position-overflow")
    data object InvalidReadRange : LedgerError("invalid-read-range")
    data object DigestMismatch : LedgerError("digest-mismatch")
    data object Corruption : LedgerError("corruption")
}

sealed interface LedgerResult<out T> {
    data class Success<T>(val value: T) : LedgerResult<T>
    data class Failure(val error: LedgerError) : LedgerResult<Nothing>
}

private fun String.isRfc3339() = try {
    OffsetDateTime.parse(this)
    true
} catch (_: DateTimeParseException) {
    false
}
