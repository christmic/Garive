package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validatePlanFact(kind: String, value: JsonObject) {
    when {
        kind == "plan.proposed" -> value.proposed()
        kind == "plan.adopted" -> value.adopted()
        kind == "plan.rejected" -> value.rejected()
        kind == "plan.superseded" -> value.superseded()
        kind == "plan.suspended" -> value.planSuspended()
        kind == "plan.resumed" -> value.planResumed()
        kind == "plan.completed" -> value.planCompleted()
        kind == "plan.failed" -> value.planFailed()
        kind.startsWith("plan.step.") -> validatePlanStepFact(kind, value)
        else -> throw IllegalArgumentException()
    }
}

private val BASE: Set<String> = setOf("command_id", "plan_id", "plan_revision")
private val VERSION: Set<String> = setOf("previous_state_version", "state_version")

private fun JsonObject.proposed() {
    exact(
        BASE + setOf(
            "state_version", "plan_digest", "definition", "goal_id", "goal_revision",
            "goal_definition_digest", "agent_snapshot_digest", "tool_catalogue_digest",
            "safety_policy_revision", "proposer_reference",
        ),
    )
    planBase()
    require(ulong("state_version") == 1uL)
    listOf(
        "plan_digest", "goal_definition_digest", "agent_snapshot_digest", "tool_catalogue_digest",
    ).forEach(::digest)
    content("definition")
    nonEmpty("goal_id")
    ulong("goal_revision", true)
    nonEmpty("safety_policy_revision")
    nonEmpty("proposer_reference")
}

private fun JsonObject.adopted() {
    planMutation(
        setOf(
            "expected_goal_revision", "actor_reference", "policy_reference", "carry_forward_evidence",
        ),
        setOf("expected_prior_plan_revision"),
    )
    ulong("expected_goal_revision", true)
    if ("expected_prior_plan_revision" in this) ulong("expected_prior_plan_revision", true)
    nonEmpty("actor_reference")
    nonEmpty("policy_reference")
    content("carry_forward_evidence")
}

private fun JsonObject.rejected() {
    planMutation(setOf("reason"))
    nonEmpty("reason")
}

private fun JsonObject.superseded() {
    planMutation(
        setOf(
            "replacement_plan_id", "replacement_plan_revision", "replacement_plan_digest", "unresolved_work",
        ),
    )
    nonEmpty("replacement_plan_id")
    ulong("replacement_plan_revision", true)
    digest("replacement_plan_digest")
    content("unresolved_work")
}

private fun JsonObject.planSuspended() {
    planMutation(setOf("continuation_kind", "continuation_reference"))
    continuation()
}

private fun JsonObject.planResumed() {
    planMutation(setOf("resolved_continuation_reference"))
    nonEmpty("resolved_continuation_reference")
}

private fun JsonObject.planCompleted() {
    planMutation(setOf("reduction_evidence"))
    content("reduction_evidence")
}

private fun JsonObject.planFailed() {
    planMutation(setOf("reason"), setOf("evidence"))
    nonEmpty("reason")
    optionalContent("evidence")
}

internal fun JsonObject.planMutation(
    additional: Set<String>,
    optional: Set<String> = emptySet(),
) {
    exact(BASE + VERSION + additional, optional)
    planBase()
    val previous = ulong("previous_state_version", true)
    require(previous != ULong.MAX_VALUE && previous + 1uL == ulong("state_version"))
}

internal fun JsonObject.continuation() {
    enum("continuation_kind", setOf("interaction", "reconciliation"))
    nonEmpty("continuation_reference")
}

private fun JsonObject.planBase() {
    nonEmpty("command_id")
    nonEmpty("plan_id")
    ulong("plan_revision", true)
}
