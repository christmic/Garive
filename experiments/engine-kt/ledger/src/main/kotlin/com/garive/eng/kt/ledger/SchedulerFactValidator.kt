package com.garive.eng.kt.ledger

import java.time.Instant
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal fun validateSchedulerFact(kind: String, value: JsonObject) {
    when (kind) {
        "schedule.created" -> value.created()
        "schedule.claimed" -> value.claimed()
        "schedule.fired" -> value.fired()
        "schedule.skipped" -> value.skipped()
        "schedule.cancelled" -> value.cancelled()
        "schedule.failed" -> value.failed()
        "schedule.exhausted" -> value.exhausted()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.exhausted() {
    exact(setOf("schedule_id", "revision_id", "last_handled_ordinal"))
    nonEmpty("schedule_id")
    nonEmpty("revision_id")
    ulong("last_handled_ordinal", true)
}

private fun JsonObject.created() {
    exact(setOf("command_id", "schedule_id", "revision_id", "intent", "intent_digest"))
    listOf("command_id", "schedule_id", "revision_id").forEach(::nonEmpty)
    content("intent")
    digest("intent_digest")
    require(text("intent_digest") == getValue("intent").jsonObject.text("digest"))
}

private fun JsonObject.claimed() {
    exact(
        setOf(
            "schedule_id", "revision_id", "occurrence_id", "ordinal", "due_at_utc", "lease_id",
            "lease_epoch", "through_position",
        ),
    )
    listOf("schedule_id", "revision_id", "occurrence_id", "lease_id").forEach(::nonEmpty)
    ulong("ordinal", true)
    timestamp("due_at_utc")
    ulong("lease_epoch", true)
    ulong("through_position")
}

private fun JsonObject.fired() {
    exact(
        setOf(
            "schedule_id", "revision_id", "occurrence_id", "ordinal", "runtime_command_id",
            "disposition", "committed_position",
        ),
    )
    listOf("schedule_id", "revision_id", "occurrence_id", "runtime_command_id").forEach(::nonEmpty)
    ulong("ordinal", true)
    enum("disposition", setOf("committed", "replayed"))
    ulong("committed_position", true)
}

private fun JsonObject.skipped() {
    exact(
        setOf(
            "schedule_id", "revision_id", "first_ordinal", "last_ordinal", "first_due_at_utc",
            "last_due_at_utc", "observed_at_utc",
        ),
    )
    listOf("schedule_id", "revision_id").forEach(::nonEmpty)
    ulong("first_ordinal", true)
    ulong("last_ordinal", true)
    require(getValue("first_ordinal").jsonPrimitive.content.toULong() <= getValue("last_ordinal").jsonPrimitive.content.toULong())
    listOf("first_due_at_utc", "last_due_at_utc", "observed_at_utc").forEach(::timestamp)
}

private fun JsonObject.cancelled() {
    exact(setOf("command_id", "schedule_id", "expected_revision_id", "reason"))
    listOf("command_id", "schedule_id", "expected_revision_id").forEach(::nonEmpty)
    enum("reason", setOf("user", "operator", "policy", "superseded"))
}

private fun JsonObject.failed() {
    exact(setOf("schedule_id", "revision_id", "reason"), setOf("occurrence_id", "ordinal"))
    nonEmpty("schedule_id")
    nonEmpty("revision_id")
    val occurrence = "occurrence_id" in this
    require(occurrence == ("ordinal" in this))
    if (occurrence) {
        nonEmpty("occurrence_id")
        ulong("ordinal", true)
    }
    enum(
        "reason",
        setOf(
            "invalid_schedule", "schedule_not_found", "revision_conflict", "subject_not_resumable",
            "authority_denied", "clock_invalid", "occurrence_overflow", "misfire_limit_exceeded",
            "lease_lost", "dispatch_conflict", "durability_failure", "corrupt_schedule_state",
        ),
    )
}

private fun JsonObject.timestamp(key: String) {
    val raw = text(key)
    require(Instant.parse(raw).toString() == raw)
}
