package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

internal fun validatePlanFact(kind: String, value: JsonObject) {
    when {
        kind == "plan.proposal.requested" -> value.proposalRequested()
        kind == "plan.proposal.result_bound" -> value.proposalResultBound()
        kind == "plan.replan.admitted" -> value.replanAdmitted()
        kind == "plan.replan.proposal.requested" -> value.replanProposalRequested()
        kind == "plan.replan.proposal.result_bound" -> value.replanProposalResultBound()
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
private val PROPOSAL_REQUEST: Set<String> = setOf(
    "command_id", "goal_id", "goal_revision", "goal_definition_digest",
    "expected_session_version", "through_position", "proposer_reference",
    "request_digest", "output_schema_digest", "turn_id", "execution_id",
)
private val PROPOSAL_RESULT: Set<String> = setOf(
    "command_id", "goal_id", "goal_revision", "goal_definition_digest",
    "request_fact_id", "planner_turn_id", "planner_execution_id", "terminal_fact_id",
    "terminal_payload_digest", "result_digest",
)
private val REPLAN_SOURCE: Set<String> = setOf(
    "admission_fact_id", "source_plan_id", "source_plan_revision",
    "source_plan_definition_digest",
)

private fun JsonObject.proposalRequested() {
    exact(PROPOSAL_REQUEST)
    proposalRequestValues()
}

private fun JsonObject.proposalResultBound() {
    exact(PROPOSAL_RESULT)
    proposalResultValues()
}

private fun JsonObject.replanProposalRequested() {
    exact(PROPOSAL_REQUEST + REPLAN_SOURCE)
    proposalRequestValues()
    replanSourceValues()
}

private fun JsonObject.replanProposalResultBound() {
    exact(PROPOSAL_RESULT + REPLAN_SOURCE)
    proposalResultValues()
    replanSourceValues()
}

private fun JsonObject.proposalRequestValues() {
    listOf("command_id", "goal_id", "proposer_reference", "turn_id", "execution_id")
        .forEach(::nonEmpty)
    ulong("goal_revision", true)
    ulong("expected_session_version", true)
    ulong("through_position")
    listOf("goal_definition_digest", "request_digest", "output_schema_digest")
        .forEach(::digest)
}

private fun JsonObject.proposalResultValues() {
    listOf(
        "command_id", "goal_id", "request_fact_id", "planner_turn_id",
        "planner_execution_id", "terminal_fact_id",
    ).forEach(::nonEmpty)
    ulong("goal_revision", true)
    listOf("goal_definition_digest", "terminal_payload_digest", "result_digest")
        .forEach(::digest)
}

private fun JsonObject.replanSourceValues() {
    nonEmpty("admission_fact_id")
    nonEmpty("source_plan_id")
    ulong("source_plan_revision", true)
    digest("source_plan_definition_digest")
}

private fun JsonObject.replanAdmitted() {
    exact(
        setOf(
            "command_id", "source_plan_id", "source_plan_revision",
            "source_plan_definition_digest", "goal_id", "goal_revision",
            "goal_definition_digest", "failed_step_ids", "policy_reference",
            "expected_session_version", "through_position", "decision_evidence",
        ),
    )
    listOf("command_id", "source_plan_id", "goal_id", "policy_reference").forEach(::nonEmpty)
    ulong("source_plan_revision", true)
    ulong("goal_revision", true)
    ulong("expected_session_version", true)
    ulong("through_position")
    digest("source_plan_definition_digest")
    digest("goal_definition_digest")
    content("decision_evidence")
    val failed = getValue("failed_step_ids") as? JsonArray ?: throw IllegalArgumentException()
    val steps = failed.map {
        (it as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull
            ?.takeIf(String::isNotEmpty) ?: throw IllegalArgumentException()
    }
    require(steps.isNotEmpty() && steps.distinct().size == steps.size)
}

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
    planMutation(setOf("actor_reference", "policy_reference", "reason"))
    nonEmpty("actor_reference")
    nonEmpty("policy_reference")
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
    enum("continuation_kind", setOf("interaction", "policy", "reconciliation"))
    nonEmpty("continuation_reference")
}

private fun JsonObject.planBase() {
    nonEmpty("command_id")
    nonEmpty("plan_id")
    ulong("plan_revision", true)
}
