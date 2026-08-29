package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validateModelFact(kind: String, value: JsonObject) {
    when (kind) {
        "model.prepared" -> value.prepared()
        "model.started" -> value.started()
        "model.completed" -> value.completed()
        "model.rejected" -> value.rejected()
        "model.interrupted" -> value.interrupted()
        "model.unavailable" -> value.unavailable()
        "model.uncertain" -> value.uncertain()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.prepared() {
    exact(setOf("request_digest", "capability_target", "deployment_id", "recovery_policy_revision", "max_attempts"))
    digest("request_digest")
    listOf("capability_target", "deployment_id", "recovery_policy_revision").forEach(::nonEmpty)
    ulong("max_attempts", true)
}

private fun JsonObject.started() {
    exact(setOf("request_digest", "dispatch_attempt_id"))
    digest("request_digest")
    nonEmpty("dispatch_attempt_id")
}

private fun JsonObject.completed() {
    exact(setOf("request_digest", "stop_reason", "items", "usage"))
    digest("request_digest")
    enum("stop_reason", setOf("end_turn", "tool_use", "stop_sequence", "pause_turn", "refusal", "other"))
    content("items")
    usage("usage")
}

private fun JsonObject.rejected() {
    exact(setOf("request_digest", "kind"), setOf("evidence"))
    digest("request_digest")
    enum("kind", setOf("context_overflow", "authentication", "content_policy"))
    optionalContent("evidence")
}

private fun JsonObject.interrupted() {
    exact(setOf("request_digest", "kind", "partial_items", "usage"))
    digest("request_digest")
    enum("kind", setOf("cancelled", "output_limit", "transport"))
    content("partial_items")
    usage("usage")
}

private fun JsonObject.unavailable() {
    exact(setOf("request_digest", "kind"), setOf("retry_after_ms"))
    digest("request_digest")
    enum("kind", setOf("rate_limited", "model_unavailable", "circuit_open"))
    optionalUlong("retry_after_ms")
}

private fun JsonObject.uncertain() {
    exact(setOf("request_digest", "reason"), setOf("evidence"))
    digest("request_digest")
    enum("reason", setOf("runtime_lost", "transport_lost", "provider_state_unknown"))
    optionalContent("evidence")
}
