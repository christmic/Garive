package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

private val delegationFailures: Set<String> = setOf(
    "invalid_delegation", "child_not_found", "child_revision_mismatch", "authority_denied",
    "budget_exhausted", "budget_overflow", "depth_exceeded", "concurrency_exceeded",
    "result_schema_mismatch", "delegation_conflict", "child_state_corrupt",
    "durability_failure", "corrupt_delegation_state",
)

internal fun validateDelegationFact(kind: String, value: JsonObject) {
    when (kind) {
        "delegation.requested" -> value.requested()
        "delegation.authorized" -> value.authorized()
        "delegation.denied" -> value.denied()
        "delegation.child_started" -> value.childStarted()
        "delegation.child_terminal" -> value.childTerminal()
        "delegation.observed" -> value.observed()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.requested() {
    exact(setOf("delegation_id", "parent_agent_instance_id", "intent", "intent_digest", "through_position"))
    nonEmpty("delegation_id"); nonEmpty("parent_agent_instance_id"); content("intent")
    digest("intent_digest"); ulong("through_position")
    require(getValue("intent").jsonObject.text("digest") == text("intent_digest"))
}

private fun JsonObject.authorized() {
    exact(setOf("delegation_id", "grant_id", "intent_digest", "reserved_budget", "authority_revision"))
    listOf("delegation_id", "grant_id", "authority_revision").forEach(::nonEmpty)
    digest("intent_digest")
    val budget = getValue("reserved_budget").jsonObject
    budget.exact(budgetFields)
    budgetFields.forEach { budget.ulong(it, true) }
    require(budget.getValue("max_child_turns").toString().toULong() == 1uL)
}

private fun JsonObject.denied() {
    exact(setOf("delegation_id", "intent_digest", "code"))
    nonEmpty("delegation_id"); digest("intent_digest"); enum("code", delegationFailures)
}

private fun JsonObject.childStarted() {
    exact(setOf("delegation_id", "grant_id", "suspension_id", "child_agent_instance_id", "child_turn_id", "child_snapshot_digest"))
    childIdentityFields.forEach(::nonEmpty); digest("child_snapshot_digest")
}

private fun JsonObject.childTerminal() {
    exact(setOf("delegation_id", "grant_id", "result_id", "suspension_id", "child_agent_instance_id", "child_turn_id", "result", "result_digest"))
    (childIdentityFields + "result_id").forEach(::nonEmpty)
    content("result"); digest("result_digest")
    require(getValue("result").jsonObject.text("digest") == text("result_digest"))
}

private fun JsonObject.observed() {
    exact(setOf("delegation_id", "grant_id", "result_id", "suspension_id", "result_digest"))
    listOf("delegation_id", "grant_id", "result_id", "suspension_id").forEach(::nonEmpty)
    digest("result_digest")
}

private val childIdentityFields: List<String> =
    listOf("delegation_id", "grant_id", "suspension_id", "child_agent_instance_id", "child_turn_id")
private val budgetFields: Set<String> = setOf(
    "max_child_turns", "max_child_executions", "max_iterations", "max_input_tokens",
    "max_output_tokens", "deadline_budget_ms", "max_depth", "max_objective_bytes",
    "max_input_evidence", "max_result_schema_bytes", "max_result_bytes", "max_result_evidence",
)
