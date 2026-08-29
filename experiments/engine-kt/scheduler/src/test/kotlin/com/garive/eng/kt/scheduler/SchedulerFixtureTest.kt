package com.garive.eng.kt.scheduler

import java.nio.file.Path
import java.time.Instant
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class SchedulerFixtureTest {
    private val root: JsonObject by lazy {
        val repo = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(repo.resolve("spec/fixtures/agent/scheduler-v1.json").readText()).jsonObject
    }

    @Test
    fun `shared intent and occurrence digests are frozen`() {
        val intent = intent(MisfirePolicy.FIRE_ONCE)
        assertEquals(root.obj("intent").text("expected_intent_digest"), intent.intentDigest().success())
        val binding = intent.intentBinding().success()
        val restored = ScheduleIntent.fromBinding("schedule-1", "revision-1", binding).success()
        assertEquals(binding, restored.intentBinding().success())
        assertEquals(
            ScheduleErrorCode.CORRUPT_SCHEDULE_STATE,
            assertIs<ScheduleContractResult.Failure>(
                ScheduleIntent.fromBinding(
                    "schedule-1",
                    "revision-1",
                    binding.copy(digest = "0".repeat(64)),
                ),
            ).code,
        )
        val first = assertIs<ScheduleDecision.Due>(
            nextOccurrence(intent, null, "2026-08-29T00:00:00Z").success(),
        ).occurrence
        assertEquals(root.obj("first_occurrence").text("expected_occurrence_id"), first.occurrenceId)
        assertEquals(root.obj("first_occurrence").text("expected_runtime_command_id"), first.runtimeCommandId)
    }

    @Test
    fun `shared recurrence and misfire cases are bounded`() {
        root.getValue("decision_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val decision = nextOccurrence(
                intent(policy(case.text("policy"))),
                case["last_handled"]?.jsonPrimitive?.content?.takeUnless { it == "null" }?.toULong(),
                case.text("now"),
            ).success()
            val actual = when (decision) {
                is ScheduleDecision.NotDue -> "not_due"
                is ScheduleDecision.Due -> "due"
                is ScheduleDecision.Skipped -> {
                    assertEquals(case.ulong("first_skipped"), decision.value.firstOrdinal)
                    assertEquals(case.ulong("last_skipped"), decision.value.lastOrdinal)
                    assertEquals(case.ulong("next_ordinal"), decision.value.nextDue?.ordinal)
                    "skipped"
                }
                ScheduleDecision.Exhausted -> "exhausted"
                is ScheduleDecision.FailMisfire -> "fail_misfire"
            }
            assertEquals(case.text("expected"), actual, case.text("name"))
        }
    }

    @Test
    fun `monotonicity and invalid matrix fail closed`() {
        val intent = intent(MisfirePolicy.FIRE_ONCE)
        var prior = ""
        repeat(5) { handled ->
            val decision = assertIs<ScheduleDecision.NotDue>(
                nextOccurrence(intent, handled.toULong(), "2026-08-28T00:00:00Z").success(),
            )
            assertTrue(decision.occurrence.dueAtUtc > prior)
            prior = decision.occurrence.dueAtUtc
        }
        assertEquals(
            ScheduleErrorCode.CLOCK_INVALID,
            assertIs<ScheduleContractResult.Failure>(nextOccurrence(intent, null, "bad-clock")).code,
        )
        val overflow = ScheduleIntent.create(
            "schedule", "revision", ScheduleSubject.START_TURN, "a".repeat(64),
            ScheduleTiming.FixedDelay("2026-08-29T00:00:00Z", ULong.MAX_VALUE, null),
            MisfirePolicy.FIRE_ONCE, 1uL, "b".repeat(64),
        ).success()
        assertEquals(
            ScheduleErrorCode.OCCURRENCE_OVERFLOW,
            assertIs<ScheduleContractResult.Failure>(
                nextOccurrence(overflow, 1uL, "2026-08-29T00:00:00Z"),
            ).code,
        )
        assertEquals(4, root.getValue("invalid_cases").jsonArray.size)
        assertEquals(
            root.getValue("failure_codes").jsonArray.map { it.jsonPrimitive.content },
            ScheduleErrorCode.entries.map { it.wireName },
        )
    }

    @Test
    fun `recurrence properties hold across delays and large misfires`() {
        listOf(1uL, 7uL, 1_000uL, UInt.MAX_VALUE.toULong()).forEach { delayMs ->
            val value = ScheduleIntent.create(
                "schedule-$delayMs", "revision", ScheduleSubject.START_TURN, "a".repeat(64),
                ScheduleTiming.FixedDelay("2026-08-29T00:00:00Z", delayMs, 64uL),
                MisfirePolicy.FIRE_ONCE, 1uL, "b".repeat(64),
            ).success()
            var previous: Instant? = null
            val identities = mutableSetOf<String>()
            (1uL..64uL).forEach { ordinal ->
                val occurrence = scheduleOccurrence(value, ordinal).success()!!
                val due = Instant.parse(occurrence.dueAtUtc)
                assertTrue(previous == null || due > previous)
                assertTrue(identities.add(occurrence.occurrenceId))
                previous = due
            }
            assertEquals(null, scheduleOccurrence(value, 65uL).success())
        }
        val unbounded = ScheduleIntent.create(
            "large-misfire", "revision", ScheduleSubject.START_TURN, "a".repeat(64),
            ScheduleTiming.FixedDelay("2026-08-29T00:00:00Z", 1uL, null),
            MisfirePolicy.SKIP, 1uL, "b".repeat(64),
        ).success()
        val skipped = assertIs<ScheduleDecision.Skipped>(
            nextOccurrence(unbounded, null, "2026-08-30T00:00:00Z").success(),
        ).value
        assertEquals(1uL, skipped.firstOrdinal)
        assertTrue(skipped.lastOrdinal > 80_000_000uL)
        assertEquals(skipped.lastOrdinal + 1uL, skipped.nextDue!!.ordinal)
    }

    private fun intent(policy: MisfirePolicy): ScheduleIntent {
        val value = root.obj("intent")
        val timing = value.obj("timing")
        return ScheduleIntent.create(
            value.text("schedule_id"), value.text("revision_id"), ScheduleSubject.START_TURN,
            value.text("subject_binding_digest"),
            ScheduleTiming.FixedDelay(
                timing.text("first_due_at_utc"), timing.ulong("delay_ms"), timing.ulong("max_occurrences"),
            ),
            policy, value.ulong("max_lateness_ms"), value.text("effective_limits_digest"),
        ).success()
    }

    private fun policy(value: String): MisfirePolicy = when (value) {
        "fire_once" -> MisfirePolicy.FIRE_ONCE
        "skip" -> MisfirePolicy.SKIP
        "fail" -> MisfirePolicy.FAIL
        else -> error("unknown fixture policy")
    }
}

private fun JsonObject.obj(key: String): JsonObject = getValue(key).jsonObject
private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.ulong(key: String): ULong = text(key).toULong()
private fun <T> ScheduleContractResult<T>.success(): T = assertIs<ScheduleContractResult.Success<T>>(this).value
