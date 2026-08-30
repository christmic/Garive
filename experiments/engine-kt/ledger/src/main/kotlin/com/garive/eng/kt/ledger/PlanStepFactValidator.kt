package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validatePlanStepFact(kind: String, value: JsonObject) {
    when (kind) {
        "plan.step.claimed" -> value.claimed()
        "plan.step.claim_expired" -> value.claimExpired()
        "plan.step.started" -> value.started()
        "plan.step.completed" -> value.completed()
        "plan.step.failed" -> value.failed()
        "plan.step.suspended" -> value.suspended()
        "plan.step.resumed" -> value.resumed()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.claimed() {
    planMutation(
        setOf(
            "step_id", "step_digest", "claim_id", "worker_reference", "lease_epoch", "clock_revision",
            "claimed_at_tick", "expires_at_tick",
        ),
    )
    nonEmpty("step_id")
    digest("step_digest")
    nonEmpty("claim_id")
    nonEmpty("worker_reference")
    ulong("lease_epoch", true)
    nonEmpty("clock_revision")
    val claimed = ulong("claimed_at_tick")
    require(ulong("expires_at_tick") > claimed)
}

private fun JsonObject.claimExpired() {
    planMutation(setOf("step_id", "claim_id", "lease_epoch", "clock_revision", "observed_at_tick"))
    nonEmpty("step_id")
    nonEmpty("claim_id")
    ulong("lease_epoch", true)
    nonEmpty("clock_revision")
    ulong("observed_at_tick")
}

private fun JsonObject.started() {
    planMutation(
        setOf(
            "step_id", "step_digest", "claim_id", "lease_epoch", "attempt_id", "execution_id",
            "execution_snapshot_digest", "sandbox_profile_digest", "safety_decision_id",
        ),
    )
    nonEmpty("step_id")
    digest("step_digest")
    nonEmpty("claim_id")
    ulong("lease_epoch", true)
    nonEmpty("attempt_id")
    nonEmpty("execution_id")
    digest("execution_snapshot_digest")
    digest("sandbox_profile_digest")
    nonEmpty("safety_decision_id")
}

private fun JsonObject.completed() {
    planMutation(
        setOf("step_id", "attempt_id", "execution_id", "result_digest", "step_evidence", "criterion_evidence"),
    )
    terminalAttempt()
    digest("result_digest")
    content("step_evidence")
    content("criterion_evidence")
}

private fun JsonObject.failed() {
    planMutation(
        setOf("step_id", "attempt_id", "execution_id", "reason", "retry_posture"),
        setOf("evidence"),
    )
    terminalAttempt()
    nonEmpty("reason")
    enum("retry_posture", setOf("retry", "suspend", "replan", "fail"))
    optionalContent("evidence")
}

private fun JsonObject.suspended() {
    planMutation(
        setOf(
            "step_id", "attempt_id", "execution_id", "continuation_kind", "continuation_reference",
        ),
    )
    terminalAttempt()
    continuation()
}

private fun JsonObject.resumed() {
    planMutation(setOf("step_id", "resolved_continuation_reference"))
    nonEmpty("step_id")
    nonEmpty("resolved_continuation_reference")
}

private fun JsonObject.terminalAttempt() {
    nonEmpty("step_id")
    nonEmpty("attempt_id")
    nonEmpty("execution_id")
}
