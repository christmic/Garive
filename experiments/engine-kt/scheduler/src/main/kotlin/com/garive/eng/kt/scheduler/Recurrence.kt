package com.garive.eng.kt.scheduler

import java.time.Duration
import java.time.Instant
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

private const val OCCURRENCE_CONTRACT = "garive.schedule-occurrence"
private const val COMMAND_CONTRACT = "garive.schedule-command"
private const val CONTRACT_VERSION = 1
private const val OCCURRENCE_PREFIX = "occurrence-"
private const val COMMAND_PREFIX = "schedule-command-"

/** One exact due occurrence with deterministic durable identities. */
public data class DueOccurrence(
    public val ordinal: ULong,
    public val dueAtUtc: String,
    public val occurrenceId: String,
    public val runtimeCommandId: String,
)

/** One bounded contiguous range produced by [MisfirePolicy.SKIP]. */
public data class SkippedOccurrences(
    public val firstOrdinal: ULong,
    public val lastOrdinal: ULong,
    public val firstDueAtUtc: String,
    public val lastDueAtUtc: String,
    public val nextDue: DueOccurrence?,
)

/** Pure deterministic reduction result for one observed clock value. */
public sealed interface ScheduleDecision {
    public data class NotDue(public val occurrence: DueOccurrence) : ScheduleDecision
    public data class Due(public val occurrence: DueOccurrence) : ScheduleDecision
    public data class Skipped(public val value: SkippedOccurrences) : ScheduleDecision
    public data object Exhausted : ScheduleDecision
    public data class FailMisfire(public val occurrence: DueOccurrence) : ScheduleDecision
}

/** Reduces immutable intent and a durable handled prefix against an explicit clock. */
public fun nextOccurrence(
    intent: ScheduleIntent,
    lastHandledOrdinal: ULong?,
    observedNowUtc: String,
): ScheduleContractResult<ScheduleDecision> {
    val now = canonicalUtc(observedNowUtc) ?: return failure(ScheduleErrorCode.CLOCK_INVALID)
    val ordinal = when (lastHandledOrdinal) {
        null -> 1uL
        ULong.MAX_VALUE -> return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
        else -> lastHandledOrdinal + 1uL
    }
    val next = when (val result = scheduleOccurrence(intent, ordinal)) {
        is ScheduleContractResult.Failure -> return result
        is ScheduleContractResult.Success -> result.value ?: return success(ScheduleDecision.Exhausted)
    }
    val due = canonicalUtc(next.dueAtUtc) ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
    if (now < due) return success(ScheduleDecision.NotDue(next))
    val lateness = intent.maxLatenessMs.toLongExact() ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
    val latest = runCatching { due.plusMillis(lateness) }.getOrNull()
        ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
    if (now <= latest) return success(ScheduleDecision.Due(next))
    return when (intent.misfirePolicy) {
        MisfirePolicy.FIRE_ONCE -> success(ScheduleDecision.Due(next))
        MisfirePolicy.FAIL -> success(ScheduleDecision.FailMisfire(next))
        MisfirePolicy.SKIP -> skipOverdue(intent, next, due, now, lateness)
    }
}

private fun skipOverdue(
    intent: ScheduleIntent,
    first: DueOccurrence,
    firstDue: Instant,
    now: Instant,
    latenessMs: Long,
): ScheduleContractResult<ScheduleDecision> {
    val maxOrdinal = when (val timing = intent.timing) {
        is ScheduleTiming.At -> 1uL
        is ScheduleTiming.FixedDelay -> timing.maxOccurrences ?: ULong.MAX_VALUE
    }
    val lastOrdinal = when (val timing = intent.timing) {
        is ScheduleTiming.At -> 1uL
        is ScheduleTiming.FixedDelay -> {
            val delay = timing.delayMs.toLongExact()
                ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
            val additional = runCatching {
                val cutoff = now.minusMillis(latenessMs)
                val deltaNs = Duration.between(firstDue, cutoff).toNanos()
                val delayNs = Math.multiplyExact(delay, 1_000_000L)
                ((deltaNs - 1L) / delayNs).toULong()
            }.getOrNull() ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
            if (ULong.MAX_VALUE - first.ordinal < additional) {
                return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
            }
            minOf(first.ordinal + additional, maxOrdinal)
        }
    }
    val last = when (val result = scheduleOccurrence(intent, lastOrdinal)) {
        is ScheduleContractResult.Failure -> return result
        is ScheduleContractResult.Success -> result.value
            ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
    }
    val next = if (lastOrdinal == maxOrdinal) {
        null
    } else {
        when (val result = scheduleOccurrence(intent, lastOrdinal + 1uL)) {
            is ScheduleContractResult.Failure -> return result
            is ScheduleContractResult.Success -> result.value
        }
    }
    return success(
        ScheduleDecision.Skipped(
            SkippedOccurrences(
                first.ordinal, lastOrdinal, first.dueAtUtc, last.dueAtUtc, next,
            ),
        ),
    )
}

/** Derives one exact ordinal's due instant and deterministic identities. */
public fun scheduleOccurrence(
    intent: ScheduleIntent,
    ordinal: ULong,
): ScheduleContractResult<DueOccurrence?> {
    val due = when (val timing = intent.timing) {
        is ScheduleTiming.At -> {
            if (ordinal != 1uL) return success(null)
            canonicalUtc(timing.dueAtUtc)!!
        }
        is ScheduleTiming.FixedDelay -> {
            if (timing.maxOccurrences?.let { ordinal > it } == true) return success(null)
            val multiplier = ordinal - 1uL
            if (timing.delayMs != 0uL && multiplier > ULong.MAX_VALUE / timing.delayMs) {
                return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
            }
            val offset = (multiplier * timing.delayMs).toLongExact()
                ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
            runCatching { canonicalUtc(timing.firstDueAtUtc)!!.plusMillis(offset) }.getOrNull()
                ?: return failure(ScheduleErrorCode.OCCURRENCE_OVERFLOW)
        }
    }
    val dueAtUtc = due.toString()
    val semantic = semantic(intent, ordinal, dueAtUtc)
    val occurrenceId = identity(OCCURRENCE_CONTRACT, OCCURRENCE_PREFIX, semantic)
        ?: return failure(ScheduleErrorCode.INVALID_SCHEDULE)
    val runtimeCommandId = identity(COMMAND_CONTRACT, COMMAND_PREFIX, semantic)
        ?: return failure(ScheduleErrorCode.INVALID_SCHEDULE)
    return success(DueOccurrence(ordinal, dueAtUtc, occurrenceId, runtimeCommandId))
}

@OptIn(ExperimentalSerializationApi::class)
private fun semantic(intent: ScheduleIntent, ordinal: ULong, dueAtUtc: String): Map<String, JsonPrimitive> = mapOf(
    "version" to JsonPrimitive(CONTRACT_VERSION),
    "schedule_id" to JsonPrimitive(intent.scheduleId),
    "revision_id" to JsonPrimitive(intent.revisionId),
    "ordinal" to JsonPrimitive(ordinal),
    "due_at_utc" to JsonPrimitive(dueAtUtc),
)

private fun identity(contract: String, prefix: String, semantic: Map<String, JsonPrimitive>): String? {
    val value = JsonObject(semantic + ("contract" to JsonPrimitive(contract)))
    return when (val result = canonicalDigest(value)) {
        is ScheduleContractResult.Success -> prefix + result.value
        is ScheduleContractResult.Failure -> null
    }
}

private fun ULong.toLongExact(): Long? = takeIf { it <= Long.MAX_VALUE.toULong() }?.toLong()
