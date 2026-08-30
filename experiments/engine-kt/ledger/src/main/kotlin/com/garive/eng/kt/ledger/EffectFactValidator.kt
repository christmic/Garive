package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validateEffectFact(kind: String, value: JsonObject) {
    when (kind) {
        "interaction.requested" -> value.interactionRequested()
        "interaction.resolved" -> value.interactionResolved()
        "interaction.cancelled" -> value.interactionCancelled()
        "tool.preparation_rejected" -> value.preparationRejected()
        "effect.prepared" -> value.prepared()
        "effect.authorized" -> value.authorized()
        "effect.denied" -> value.denied()
        "effect.started" -> value.started()
        "effect.receipt" -> value.receipt()
        "effect.completed" -> value.completed()
        "effect.failed" -> value.failed()
        "effect.uncertain" -> value.uncertain()
        "effect.reconciled" -> value.reconciled()
        "effect.observation" -> value.observation()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.interactionRequested() {
    exact(
        setOf(
            "interaction_id",
            "suspension_id",
            "prepared_digest",
            "kind",
            "prompt",
            "response_schema",
            "response_schema_digest",
            "expiry_code",
        ),
    )
    identities("interaction_id", "suspension_id")
    digests("prepared_digest", "response_schema_digest")
    enum("kind", setOf("approval", "external_input"))
    enum("expiry_code", setOf("none", "turn_deadline", "policy_deadline"))
    content("prompt")
    content("response_schema")
}

private fun JsonObject.interactionResolved() {
    exact(setOf("interaction_id", "suspension_id", "prepared_digest", "response"))
    identities("interaction_id", "suspension_id")
    digest("prepared_digest")
    content("response")
}

private fun JsonObject.interactionCancelled() {
    exact(setOf("interaction_id", "suspension_id", "prepared_digest", "reason"))
    identities("interaction_id", "suspension_id")
    digest("prepared_digest")
    enum("reason", setOf("user", "expired", "turn_cancelled", "operator"))
}

private fun JsonObject.preparationRejected() {
    exact(setOf("source_model_request_id", "model_call_id", "proposed_tool_name", "code", "failure_paths"))
    identities("source_model_request_id", "model_call_id")
    text("proposed_tool_name")
    enum("code", setOf("invalid_tool_name", "tool_not_admitted", "invalid_arguments_json", "arguments_schema_mismatch", "non_canonical_value"))
    content("failure_paths")
}

private fun JsonObject.prepared() {
    exact(setOf("prepared_digest", "tool_name", "tool_revision", "replay_class", "model_call_id"))
    digest("prepared_digest")
    identities("tool_name", "tool_revision", "model_call_id")
    enum("replay_class", setOf("read_only", "idempotent", "receipt_recoverable", "never_replay"))
}

private fun JsonObject.authorized() {
    exact(setOf("prepared_digest", "grant_id", "authority_revision", "granted_requirements"))
    digest("prepared_digest")
    identities("grant_id", "authority_revision")
    content("granted_requirements")
}

private fun JsonObject.denied() {
    exact(setOf("prepared_digest", "code"), setOf("safe_details"))
    digest("prepared_digest")
    enum("code", setOf("authorization_denied", "replacement_required"))
    optionalContent("safe_details")
}

private fun JsonObject.started() {
    exact(setOf("prepared_digest", "grant_id", "executor_id", "executor_revision", "dispatch_attempt_id"))
    digest("prepared_digest")
    identities("grant_id", "executor_id", "executor_revision", "dispatch_attempt_id")
}

private fun JsonObject.receipt() {
    exact(setOf("receipt_id", "prepared_digest", "grant_id", "executor_id", "executor_revision", "classification", "result_or_evidence"))
    digest("prepared_digest")
    identities("receipt_id", "grant_id", "executor_id", "executor_revision")
    enum("classification", setOf("completed", "failed"))
    content("result_or_evidence")
}

private fun JsonObject.completed() {
    exact(setOf("prepared_digest", "receipt_id", "result"))
    digest("prepared_digest")
    nonEmpty("receipt_id")
    content("result")
}

private fun JsonObject.failed() {
    exact(setOf("prepared_digest", "code"), setOf("receipt_id", "evidence"))
    digest("prepared_digest")
    optionalNonEmpty("receipt_id")
    enum("code", setOf("timeout", "cancelled", "tool_failure", "requirement_unsupported", "executor_unavailable"))
    optionalContent("evidence")
}

private fun JsonObject.uncertain() {
    exact(setOf("prepared_digest", "reason"), setOf("evidence"))
    digest("prepared_digest")
    enum("reason", setOf("started_without_receipt", "receipt_invalid", "executor_state_unknown"))
    optionalContent("evidence")
}

private fun JsonObject.reconciled() {
    exact(setOf("prepared_digest", "decision", "operator_evidence", "observation"))
    digest("prepared_digest")
    enum("decision", setOf("completed", "failed"))
    content("operator_evidence")
    content("observation")
}

private fun JsonObject.observation() {
    exact(setOf("prepared_digest", "model_call_id", "observation"))
    digest("prepared_digest")
    nonEmpty("model_call_id")
    content("observation")
}

private fun JsonObject.identities(vararg keys: String) = keys.forEach(::nonEmpty)
private fun JsonObject.digests(vararg keys: String) = keys.forEach(::digest)
