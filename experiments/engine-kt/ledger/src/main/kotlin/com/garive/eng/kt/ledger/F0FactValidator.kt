package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validateF0Fact(kind: String, value: JsonObject) {
    when (kind) {
        "safety.decided" -> value.safetyDecided()
        "sandbox.bound" -> value.sandboxBound()
        "sandbox.preflighted" -> value.sandboxPreflighted()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.safetyDecided() {
    exact(
        setOf(
            "request_id", "decision_id", "disposition", "prepared_digest", "tool_name",
            "tool_revision", "actor_authority_reference", "exact_access_digest",
            "sandbox_requirements_digest", "policy_revision",
        ),
        setOf("goal_reference", "plan_reference", "constraints_digest", "safe_code"),
    )
    identities(
        "request_id", "decision_id", "tool_name", "tool_revision",
        "actor_authority_reference", "policy_revision",
    )
    optionalNonEmpty("goal_reference")
    optionalNonEmpty("plan_reference")
    digests("prepared_digest", "exact_access_digest", "sandbox_requirements_digest")
    when (enum("disposition", setOf("allow", "deny", "interaction_required"))) {
        "allow" -> {
            require("safe_code" !in this)
            digest("constraints_digest")
        }
        "deny" -> {
            require("constraints_digest" !in this && text("safe_code") == "safety_denied")
        }
        else -> require(
            "constraints_digest" !in this && text("safe_code") == "safety_interaction_required",
        )
    }
}

private fun JsonObject.sandboxBound() {
    exact(
        setOf(
            "binding_id", "decision_id", "prepared_digest", "workspace_capability_id",
            "executor_id", "executor_revision", "policy_revision", "access_scope_digest",
            "enforcement_digest", "effective_limits_digest",
        ),
    )
    identities(
        "binding_id", "decision_id", "workspace_capability_id", "executor_id",
        "executor_revision", "policy_revision",
    )
    digests("prepared_digest", "access_scope_digest", "enforcement_digest", "effective_limits_digest")
}

private fun JsonObject.sandboxPreflighted() {
    exact(
        setOf(
            "preflight_id", "binding_id", "decision_id", "prepared_digest", "grant_id",
            "executor_id", "executor_revision", "dispatch_attempt_id",
        ),
    )
    identities(
        "preflight_id", "binding_id", "decision_id", "grant_id", "executor_id",
        "executor_revision", "dispatch_attempt_id",
    )
    digest("prepared_digest")
}

private fun JsonObject.identities(vararg keys: String) = keys.forEach(::nonEmpty)
private fun JsonObject.digests(vararg keys: String) = keys.forEach(::digest)
