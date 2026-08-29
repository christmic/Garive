package com.garive.eng.kt.scheduler

import java.security.MessageDigest
import java.time.Instant
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

private const val INTENT_CONTRACT = "garive.schedule-intent"
private const val CONTRACT_VERSION = 1
private const val MAX_ID_BYTES = 128

/** Stable Q0 validation, recurrence, authority, lease, or durability failure. */
public enum class ScheduleErrorCode(public val wireName: String) {
    INVALID_SCHEDULE("invalid_schedule"),
    SCHEDULE_NOT_FOUND("schedule_not_found"),
    REVISION_CONFLICT("revision_conflict"),
    SUBJECT_NOT_RESUMABLE("subject_not_resumable"),
    AUTHORITY_DENIED("authority_denied"),
    CLOCK_INVALID("clock_invalid"),
    OCCURRENCE_OVERFLOW("occurrence_overflow"),
    MISFIRE_LIMIT_EXCEEDED("misfire_limit_exceeded"),
    LEASE_LOST("lease_lost"),
    DISPATCH_CONFLICT("dispatch_conflict"),
    DURABILITY_FAILURE("durability_failure"),
    CORRUPT_SCHEDULE_STATE("corrupt_schedule_state"),
}

/** Typed portable Scheduler result. */
public sealed interface ScheduleContractResult<out T> {
    public data class Success<T>(public val value: T) : ScheduleContractResult<T>
    public data class Failure(public val code: ScheduleErrorCode) : ScheduleContractResult<Nothing>
}

/** Portable scheduled command subject. */
public enum class ScheduleSubject(public val wireName: String) {
    START_TURN("start_turn"),
    CONTINUE_TURN_RESOURCE_READY("continue_turn_resource_ready"),
}

/** Portable timing without timezone or calendar semantics. */
public sealed interface ScheduleTiming {
    public data class At(public val dueAtUtc: String) : ScheduleTiming
    public data class FixedDelay(
        public val firstDueAtUtc: String,
        public val delayMs: ULong,
        public val maxOccurrences: ULong?,
    ) : ScheduleTiming
}

/** Portable overdue-occurrence policy. */
public enum class MisfirePolicy(public val wireName: String) {
    FIRE_ONCE("fire_once"),
    SKIP("skip"),
    FAIL("fail"),
}

/** Canonical inline schedule intent committed by Runtime. */
public data class ScheduleIntentBinding(public val digest: String, public val inlineUtf8: String)

/** Exact immutable portable schedule semantics. */
public class ScheduleIntent private constructor(
    public val scheduleId: String,
    public val revisionId: String,
    public val subject: ScheduleSubject,
    public val subjectBindingDigest: String,
    public val timing: ScheduleTiming,
    public val misfirePolicy: MisfirePolicy,
    public val maxLatenessMs: ULong,
    public val effectiveLimitsDigest: String,
) {
    /** Computes RFC 8785 SHA-256 over portable intent semantics. */
    public fun intentDigest(): ScheduleContractResult<String> = when (val binding = intentBinding()) {
        is ScheduleContractResult.Success -> success(binding.value.digest)
        is ScheduleContractResult.Failure -> binding
    }

    /** Returns the canonical inline binding for `schedule.created`. */
    public fun intentBinding(): ScheduleContractResult<ScheduleIntentBinding> = runCatching {
        val bytes = JsonCanonicalizer(intentJson().toString()).encodedUTF8
        val text = bytes.decodeToString()
        ScheduleIntentBinding(sha256(bytes), text)
    }.fold(::success) { failure(ScheduleErrorCode.INVALID_SCHEDULE) }

    public companion object {
        /** Validates identities, digests, canonical UTC timestamps and non-zero bounds. */
        @Suppress("LongParameterList")
        public fun create(
            scheduleId: String,
            revisionId: String,
            subject: ScheduleSubject,
            subjectBindingDigest: String,
            timing: ScheduleTiming,
            misfirePolicy: MisfirePolicy,
            maxLatenessMs: ULong,
            effectiveLimitsDigest: String,
        ): ScheduleContractResult<ScheduleIntent> {
            val validTiming = when (timing) {
                is ScheduleTiming.At -> canonicalUtc(timing.dueAtUtc) != null
                is ScheduleTiming.FixedDelay -> canonicalUtc(timing.firstDueAtUtc) != null &&
                    timing.delayMs != 0uL && timing.maxOccurrences != 0uL
            }
            return if (!validId(scheduleId) || !validId(revisionId) ||
                !validDigest(subjectBindingDigest) || !validTiming || maxLatenessMs == 0uL ||
                !validDigest(effectiveLimitsDigest)
            ) {
                failure(ScheduleErrorCode.INVALID_SCHEDULE)
            } else {
                success(
                    ScheduleIntent(
                        scheduleId, revisionId, subject, subjectBindingDigest, timing,
                        misfirePolicy, maxLatenessMs, effectiveLimitsDigest,
                    ),
                )
            }
        }

        /** Reconstructs and verifies one persisted canonical inline intent binding. */
        public fun fromBinding(
            scheduleId: String,
            revisionId: String,
            binding: ScheduleIntentBinding,
        ): ScheduleContractResult<ScheduleIntent> = runCatching {
            val value = Json.parseToJsonElement(binding.inlineUtf8).jsonObject
            require(value.keys == INTENT_FIELDS)
            val canonical = JsonCanonicalizer(value.toString()).encodedUTF8
            require(canonical.decodeToString() == binding.inlineUtf8)
            require(sha256(canonical) == binding.digest)
            require(value.text("contract") == INTENT_CONTRACT)
            require(value.text("version").toInt() == CONTRACT_VERSION)
            val subject = ScheduleSubject.entries.single { it.wireName == value.text("subject") }
            val policy = MisfirePolicy.entries.single { it.wireName == value.text("misfire_policy") }
            val timing = parseTiming(value.getValue("timing").jsonObject)
            when (
                val created = create(
                    scheduleId, revisionId, subject, value.text("subject_binding_digest"), timing,
                    policy, value.text("max_lateness_ms").toULong(),
                    value.text("effective_limits_digest"),
                )
            ) {
                is ScheduleContractResult.Success -> created.value
                is ScheduleContractResult.Failure -> error(created.code.wireName)
            }
        }.fold(::success) { failure(ScheduleErrorCode.CORRUPT_SCHEDULE_STATE) }
    }

    @OptIn(ExperimentalSerializationApi::class)
    private fun intentJson(): JsonObject = JsonObject(
        mapOf(
            "contract" to JsonPrimitive(INTENT_CONTRACT),
            "version" to JsonPrimitive(CONTRACT_VERSION),
            "subject" to JsonPrimitive(subject.wireName),
            "subject_binding_digest" to JsonPrimitive(subjectBindingDigest),
            "timing" to timingJson(timing),
            "misfire_policy" to JsonPrimitive(misfirePolicy.wireName),
            "max_lateness_ms" to JsonPrimitive(maxLatenessMs),
            "effective_limits_digest" to JsonPrimitive(effectiveLimitsDigest),
        ),
    )
}

private val INTENT_FIELDS = setOf(
    "contract", "version", "subject", "subject_binding_digest", "timing",
    "misfire_policy", "max_lateness_ms", "effective_limits_digest",
)

private fun parseTiming(value: JsonObject): ScheduleTiming = when (value.text("kind")) {
    "at" -> {
        require(value.keys == setOf("kind", "due_at_utc"))
        ScheduleTiming.At(value.text("due_at_utc"))
    }
    "fixed_delay" -> {
        require(
            value.keys == setOf("kind", "first_due_at_utc", "delay_ms") ||
                value.keys == setOf("kind", "first_due_at_utc", "delay_ms", "max_occurrences"),
        )
        ScheduleTiming.FixedDelay(
            value.text("first_due_at_utc"),
            value.text("delay_ms").toULong(),
            value["max_occurrences"]?.jsonPrimitive?.content?.toULong(),
        )
    }
    else -> error("timing kind")
}

private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content

@OptIn(ExperimentalSerializationApi::class)
private fun timingJson(value: ScheduleTiming): JsonObject = when (value) {
    is ScheduleTiming.At -> JsonObject(
        mapOf("kind" to JsonPrimitive("at"), "due_at_utc" to JsonPrimitive(value.dueAtUtc)),
    )
    is ScheduleTiming.FixedDelay -> JsonObject(
        buildMap {
            put("kind", JsonPrimitive("fixed_delay"))
            put("first_due_at_utc", JsonPrimitive(value.firstDueAtUtc))
            put("delay_ms", JsonPrimitive(value.delayMs))
            value.maxOccurrences?.let { put("max_occurrences", JsonPrimitive(it)) }
        },
    )
}

internal fun canonicalUtc(value: String): Instant? =
    runCatching { Instant.parse(value).takeIf { it.toString() == value } }.getOrNull()

internal fun canonicalDigest(value: JsonObject): ScheduleContractResult<String> = runCatching {
    sha256(JsonCanonicalizer(value.toString()).encodedUTF8)
}.fold(::success) { failure(ScheduleErrorCode.INVALID_SCHEDULE) }

internal fun sha256(value: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }

private fun validId(value: String): Boolean =
    value.isNotEmpty() && value.encodeToByteArray().size <= MAX_ID_BYTES && value.trim() == value

private fun validDigest(value: String): Boolean = value.matches(Regex("[0-9a-f]{64}"))
internal fun <T> success(value: T): ScheduleContractResult.Success<T> = ScheduleContractResult.Success(value)
internal fun failure(code: ScheduleErrorCode): ScheduleContractResult.Failure = ScheduleContractResult.Failure(code)
