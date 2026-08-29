package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validateTurnFact(kind: String, value: JsonObject) {
    when (kind) {
        "turn.started" -> value.started()
        "turn.input" -> value.input()
        "turn.cancel_requested" -> value.cancel()
        "turn.suspended" -> value.suspended(true)
        "turn.completed" -> value.completed(true)
        "turn.stopped" -> value.stopped(true)
        "turn.failed" -> value.failed(true)
        "execution.started" -> value.executionStarted()
        "execution.iteration_started" -> value.iterationStarted()
        "execution.abandoned" -> value.abandoned()
        "execution.completed" -> value.completed(false)
        "execution.suspended" -> value.suspended(false)
        "execution.stopped" -> value.stopped(false)
        "execution.failed" -> value.failed(false)
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.started() {
    exact(
        setOf("command_id", "kind", "agent_instance_id", "definition_id", "definition_revision", "snapshot_digest", "trusted_input_digest"),
        setOf("prior_suspension_id", "expected_session_version"),
    )
    listOf("command_id", "agent_instance_id", "definition_id", "definition_revision").forEach(::nonEmpty)
    digest("snapshot_digest")
    digest("trusted_input_digest")
    conditionalIdentity("kind", "continue", "prior_suspension_id", setOf("start", "continue"))
    val continuation = enum("kind", setOf("start", "continue")) == "continue"
    require(continuation == containsKey("expected_session_version"))
    if (continuation) ulong("expected_session_version", nonzero = true)
}

private fun JsonObject.input() {
    exact(setOf("input_kind", "content"), setOf("suspension_id"))
    content("content")
    val kind = enum(
        "input_kind",
        setOf("trusted_user", "trusted_system", "external_input", "reconciliation", "resource_ready"),
    )
    val continuation = kind != "trusted_user" && kind != "trusted_system"
    require(continuation == containsKey("suspension_id"))
    if (continuation) nonEmpty("suspension_id")
}

private fun JsonObject.conditionalIdentity(enumKey: String, requiring: String, identity: String, allowed: Set<String>) {
    val required = enum(enumKey, allowed) == requiring
    require(required == containsKey(identity))
    if (required) nonEmpty(identity)
}

private fun JsonObject.cancel() {
    exact(setOf("command_id", "reason", "requested_through_position"))
    nonEmpty("command_id")
    enum("reason", setOf("user", "deadline", "shutdown", "operator", "policy"))
    ulong("requested_through_position")
}

private fun JsonObject.suspended(turn: Boolean) {
    exact(
        if (turn) setOf("suspension_id", "execution_id", "reason", "continuation", "cumulative_usage")
        else setOf("suspension_id", "reason", "continuation", "usage", "completed_iterations"),
    )
    nonEmpty("suspension_id")
    if (turn) nonEmpty("execution_id") else ulong("completed_iterations")
    enum("reason", setOf("approval_required", "external_input_required", "operator_reconciliation", "resource_unavailable", "partial_output"))
    content("continuation")
    usage(if (turn) "cumulative_usage" else "usage")
}

private fun JsonObject.completed(turn: Boolean) {
    exact(if (turn) setOf("execution_id", "response", "cumulative_usage") else setOf("response", "usage", "completed_iterations"))
    if (turn) nonEmpty("execution_id") else ulong("completed_iterations")
    content("response")
    usage(if (turn) "cumulative_usage" else "usage")
}

private fun JsonObject.stopped(turn: Boolean) {
    exact(
        if (turn) setOf("execution_id", "reason", "cumulative_usage") else setOf("reason", "usage", "completed_iterations"),
        setOf("evidence"),
    )
    if (turn) nonEmpty("execution_id") else ulong("completed_iterations")
    enum("reason", setOf("iteration_limit", "token_limit", "deadline", "cancelled", "resource_unavailable"))
    optionalContent("evidence")
    usage(if (turn) "cumulative_usage" else "usage")
}

private fun JsonObject.failed(turn: Boolean) {
    exact(
        if (turn) setOf("execution_id", "reason", "cumulative_usage") else setOf("reason", "usage", "completed_iterations"),
        setOf("evidence"),
    )
    if (turn) nonEmpty("execution_id") else ulong("completed_iterations")
    enum("reason", setOf("invalid_input", "invalid_model_output", "required_capability_unavailable", "port_failure", "invariant_violation", "durability_failure", "corrupt_recovery_state"))
    optionalContent("evidence")
    usage(if (turn) "cumulative_usage" else "usage")
}

private fun JsonObject.executionStarted() {
    exact(setOf("snapshot_digest", "through_position", "completed_iterations", "limits", "recovery_ordinal"))
    digest("snapshot_digest")
    listOf("through_position", "completed_iterations", "recovery_ordinal").forEach(::ulong)
    limits("limits")
}

private fun JsonObject.iterationStarted() {
    exact(setOf("iteration"))
    ulong("iteration", true)
}

private fun JsonObject.abandoned() {
    exact(setOf("reason", "last_safe_position", "recovery_ordinal"))
    enum("reason", setOf("runtime_lost"))
    ulong("last_safe_position")
    ulong("recovery_ordinal", true)
}
