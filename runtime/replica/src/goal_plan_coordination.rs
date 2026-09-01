use std::collections::BTreeSet;

use garive_goal::{GoalEvidenceV1, GoalState};
use garive_ledger::{CanonicalPayload, DurableFact, SessionId, TurnId};
use garive_plan::{PlanDefinitionV1, PlanState};
use serde_json::{json, Value};

use crate::plan_runtime::{plan_resume_step_execution, plan_suspend_step_and_plan};
use crate::{
    get_turn, plan_cancel_turn, plan_complete_plan, plan_goal_transition, plan_plan_transition,
    reconstruct_goal, reconstruct_plan_graph, reconstruct_suspended_turn, CancelReason,
    CancelTurnCommand, GetTurnQuery, GoalCommandContext, GoalRuntimeError, GoalRuntimeTransition,
    PlanCommandContext, PlanRetryPosture, PlanRuntimeError, PlanRuntimeTransition,
    PlanStepContinuation, PlanStepSuspension, PlannedGoalCommand, PlannedPlanCommand, PlannedTurn,
    RuntimeCommandError, RuntimeCommandId, RuntimeSuspensionKind, RuntimeTurnStatus, SqliteLedger,
};

/// Stable failure classes for cross-aggregate Goal/Plan coordination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalPlanCoordinationError {
    /// The expected Session prefix changed during evaluation.
    ConcurrentModification,
    /// No unique adopted non-terminal Plan binds the exact Goal revision.
    AuthoritativePlanUnavailable,
    /// No unique completed Plan binds the exact Goal revision.
    CompletedPlanUnavailable,
    /// No unique ledger-proven resumable Turn can suspend the Goal.
    ResumableSuspensionUnavailable,
    /// No unique durable continuation can resume the Goal's suspension.
    ContinuationUnavailable,
    /// The owned Turn has no unique completed terminal to reduce.
    CompletedTurnUnavailable,
    /// Goal planning rejected the derived activation.
    Goal(GoalRuntimeError),
    /// Durable Turn planning rejected a derived cancellation.
    Runtime(RuntimeCommandError),
    /// Durable Plan planning rejected a derived suspension or continuation.
    Plan(PlanRuntimeError),
    /// Plan recovery or canonical reference derivation failed closed.
    CorruptState,
}

/// One deterministic cancellation propagation step for a Goal-owned Turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedGoalTurnCancellation {
    /// Session version against which this one-fact command must commit.
    pub expected_session_version: u64,
    /// Goal-owned Turn selected in stable identity order.
    pub turn_id: TurnId,
    /// Exact idempotent C6 cancellation request.
    pub planned: PlannedTurn,
}

/// Plans one owned Step completion from the exact committed Turn terminal.
///
/// Result and criterion evidence are re-observed at the current ledger prefix;
/// the caller supplies no Step, attempt, Execution, result or evidence binding.
pub fn plan_complete_owned_step_from_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    turn_id: &TurnId,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &PlanCommandContext,
) -> Result<PlannedPlanCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Active {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    if goal.session_version != expected_session_version {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut candidates = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && plan.snapshot.state() == PlanState::Running
    });
    let plan = candidates
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if candidates.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if !owned_turns(&facts, plan.snapshot.definition())?.contains(turn_id) {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let view = get_turn(
        ledger,
        &GetTurnQuery {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            through_position: None,
        },
    )
    .map_err(GoalPlanCoordinationError::Runtime)?;
    if view.status != RuntimeTurnStatus::Completed
        || view.observed_session_version != expected_session_version
    {
        return Err(GoalPlanCoordinationError::CompletedTurnUnavailable);
    }
    let execution_id = view
        .execution_id
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let claims = plan
        .active_claims
        .iter()
        .filter(|(_, claim)| claim.execution_id.as_deref() == Some(execution_id.as_str()))
        .collect::<Vec<_>>();
    let [(step_id, claim)] = claims.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let step = plan
        .snapshot
        .definition()
        .steps()
        .iter()
        .find(|step| step.step_id() == *step_id)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let criteria = goal
        .snapshot
        .definition()
        .criteria()
        .iter()
        .filter(|criterion| {
            step.completion_criteria()
                .contains(criterion.criterion_id().as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    if criteria.len() != step.completion_criteria().len() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let graph = crate::goal_recovery::reconstruct_goal_graph_from_facts(
        &facts,
        expected_session_version,
        goal.through_position,
    )
    .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let criterion_evidence = crate::goal_evidence::observe_goal_evidence(
        goal_id,
        &criteria,
        &graph,
        &facts,
        expected_session_version,
    )
    .map_err(GoalPlanCoordinationError::Goal)?;
    let terminals = facts
        .iter()
        .filter(|fact| {
            fact.kind.as_str() == "turn.completed" && fact.turn_id.as_ref() == Some(turn_id)
        })
        .collect::<Vec<_>>();
    let [terminal] = terminals.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let terminal_payload = serde_json::from_str::<Value>(terminal.payload.as_json())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if terminal_payload.get("execution_id").and_then(Value::as_str) != Some(execution_id.as_str()) {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let result_digest = terminal_payload
        .get("response")
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("digest"))
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let terminal_commit_version = ledger
        .fact_commit_version(&terminal.fact_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let execution_terminals = facts
        .iter()
        .filter(|fact| {
            fact.kind.as_str() == "execution.completed"
                && fact.turn_id.as_ref() == Some(turn_id)
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .collect::<Vec<_>>();
    let [execution_terminal] = execution_terminals.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let execution_commit_version = ledger
        .fact_commit_version(&execution_terminal.fact_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let execution_payload = serde_json::from_str::<Value>(execution_terminal.payload.as_json())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if execution_commit_version != terminal_commit_version
        || execution_payload
            .get("response")
            .and_then(Value::as_object)
            .and_then(|binding| binding.get("digest"))
            .and_then(Value::as_str)
            != Some(result_digest)
    {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let step_evidence = CanonicalPayload::from_value(&json!({
        "contract":"garive.plan-step-evidence",
        "version":1,
        "terminal_commit_version":terminal_commit_version,
        "terminal_fact_id":terminal.fact_id.as_str(),
        "terminal_payload_digest":terminal.payload.sha256(),
        "terminal_position":terminal.position,
        "turn_id":turn_id.as_str(),
    }))
    .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let criterion_json = GoalEvidenceV1::canonical_json(&criterion_evidence)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let criterion_evidence = CanonicalPayload::from_value(
        &serde_json::from_str::<Value>(&criterion_json)
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?,
    )
    .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    plan_plan_transition(
        plan,
        plan.state_version,
        context,
        PlanRuntimeTransition::CompleteStep {
            step_id: (*step_id).clone(),
            attempt_id: claim
                .attempt_id
                .clone()
                .ok_or(GoalPlanCoordinationError::CorruptState)?,
            execution_id: execution_id.as_str().into(),
            result_digest: result_digest.into(),
            step_evidence,
            criterion_evidence,
        },
    )
    .map_err(GoalPlanCoordinationError::Plan)
}

/// Plans one owned Step failure from the exact committed Turn terminal.
///
/// Runtime derives both the stable reason and bounded retry posture. Cancelled
/// Turns are not failures and are rejected from this path.
pub fn plan_fail_owned_step_from_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    turn_id: &TurnId,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &PlanCommandContext,
) -> Result<PlannedPlanCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Active
        || goal.session_version != expected_session_version
        || goal.snapshot.revision() != expected_goal_revision
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut candidates = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && plan.snapshot.state() == PlanState::Running
    });
    let plan = candidates
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if candidates.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if !owned_turns(&facts, plan.snapshot.definition())?.contains(turn_id) {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let view = get_turn(
        ledger,
        &GetTurnQuery {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            through_position: None,
        },
    )
    .map_err(GoalPlanCoordinationError::Runtime)?;
    let (execution_kind, turn_kind, class) = match view.status {
        RuntimeTurnStatus::Failed => ("execution.failed", "turn.failed", "failed"),
        RuntimeTurnStatus::Stopped => ("execution.stopped", "turn.stopped", "stopped"),
        _ => return Err(GoalPlanCoordinationError::CompletedTurnUnavailable),
    };
    if view.observed_session_version != expected_session_version {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let execution_id = view
        .execution_id
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let claims = plan
        .active_claims
        .iter()
        .filter(|(_, claim)| claim.execution_id.as_deref() == Some(execution_id.as_str()))
        .collect::<Vec<_>>();
    let [(step_id, claim)] = claims.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let terminals = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == turn_kind && fact.turn_id.as_ref() == Some(turn_id))
        .collect::<Vec<_>>();
    let executions = facts
        .iter()
        .filter(|fact| {
            fact.kind.as_str() == execution_kind
                && fact.turn_id.as_ref() == Some(turn_id)
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .collect::<Vec<_>>();
    let ([terminal], [execution]) = (terminals.as_slice(), executions.as_slice()) else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let terminal_payload = serde_json::from_str::<Value>(terminal.payload.as_json())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let execution_payload = serde_json::from_str::<Value>(execution.payload.as_json())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let reason = terminal_payload
        .get("reason")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    if terminal_payload.get("execution_id").and_then(Value::as_str) != Some(execution_id.as_str())
        || execution_payload.get("reason").and_then(Value::as_str) != Some(reason)
        || reason == "cancelled"
    {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let terminal_version = ledger
        .fact_commit_version(&terminal.fact_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    if ledger
        .fact_commit_version(&execution.fact_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?
        != Some(terminal_version)
    {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let evidence = CanonicalPayload::from_value(&json!({
        "contract":"garive.plan-step-failure-evidence",
        "version":1,
        "execution_fact_id":execution.fact_id.as_str(),
        "execution_payload_digest":execution.payload.sha256(),
        "terminal_commit_version":terminal_version,
        "turn_fact_id":terminal.fact_id.as_str(),
        "turn_payload_digest":terminal.payload.sha256(),
        "turn_id":turn_id.as_str(),
    }))
    .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let stable_reason = format!("{class}_{reason}");
    let retryable = matches!(
        reason,
        "invalid_model_output" | "port_failure" | "resource_unavailable"
    );
    let attempt_id = claim
        .attempt_id
        .clone()
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let transition = |retry_posture| PlanRuntimeTransition::FailStep {
        step_id: (*step_id).clone(),
        attempt_id: attempt_id.clone(),
        execution_id: execution_id.as_str().into(),
        reason: stable_reason.clone(),
        evidence: Some(evidence.clone()),
        retry_posture,
    };
    if retryable {
        match plan_plan_transition(
            plan,
            plan.state_version,
            context,
            transition(PlanRetryPosture::Retry),
        ) {
            Ok(planned) => return Ok(planned),
            Err(PlanRuntimeError::BoundExceeded) => {}
            Err(error) => return Err(GoalPlanCoordinationError::Plan(error)),
        }
    }
    plan_plan_transition(
        plan,
        plan.state_version,
        context,
        transition(PlanRetryPosture::Fail),
    )
    .map_err(GoalPlanCoordinationError::Plan)
}

/// Plans the authoritative Plan terminal when every Step is durably complete.
///
/// `None` means the authoritative Plan still has unfinished Steps. Complete
/// Goal evidence is observed and verified at the current ledger prefix.
pub fn plan_complete_authoritative_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &PlanCommandContext,
) -> Result<Option<PlannedPlanCommand>, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Active {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    if goal.session_version != expected_session_version {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut candidates = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && plan.snapshot.state() == PlanState::Running
    });
    let plan = candidates
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if candidates.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    if plan.snapshot.definition().steps().iter().any(|step| {
        plan.snapshot
            .step(step.step_id())
            .map(|value| value.state())
            != Some(garive_plan::StepState::Completed)
    }) {
        return Ok(None);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let graph = crate::goal_recovery::reconstruct_goal_graph_from_facts(
        &facts,
        expected_session_version,
        goal.through_position,
    )
    .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let evidence = crate::goal_evidence::observe_goal_evidence(
        goal_id,
        goal.snapshot.definition().criteria(),
        &graph,
        &facts,
        expected_session_version,
    )
    .map_err(GoalPlanCoordinationError::Goal)?;
    plan_complete_plan(
        ledger,
        session_id,
        plan,
        plan.state_version,
        context,
        evidence,
    )
    .map(Some)
    .map_err(GoalPlanCoordinationError::Plan)
}

/// Plans Step and Plan suspension from one ledger-proven owned Turn terminal.
pub fn plan_suspend_owned_plan_from_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &PlanCommandContext,
) -> Result<PlannedPlanCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Active {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if goal.session_version != expected_session_version
        || plans.values().any(|plan| {
            plan.session_version != goal.session_version
                || plan.through_position != goal.through_position
        })
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut candidates = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == digest
            && plan.snapshot.state() == PlanState::Running
    });
    let plan = candidates
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if candidates.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut suspended = Vec::new();
    for turn_id in owned_turns(&facts, plan.snapshot.definition())? {
        let view = get_turn(
            ledger,
            &GetTurnQuery {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                through_position: None,
            },
        )
        .map_err(GoalPlanCoordinationError::Runtime)?;
        if view.status == RuntimeTurnStatus::Open {
            return Err(GoalPlanCoordinationError::ResumableSuspensionUnavailable);
        }
        if view.status == RuntimeTurnStatus::Suspended {
            let state = reconstruct_suspended_turn(
                &ledger
                    .load_turn(&turn_id)
                    .map_err(|_| GoalPlanCoordinationError::CorruptState)?,
            )
            .map_err(GoalPlanCoordinationError::Runtime)?;
            let execution_id = view
                .execution_id
                .ok_or(GoalPlanCoordinationError::CorruptState)?;
            suspended.push((state, execution_id));
        }
    }
    let [(suspension, execution_id)] = suspended.as_slice() else {
        return Err(GoalPlanCoordinationError::ResumableSuspensionUnavailable);
    };
    let claims = plan
        .active_claims
        .iter()
        .filter(|(_, claim)| claim.execution_id.as_deref() == Some(execution_id.as_str()))
        .collect::<Vec<_>>();
    let [(step_id, claim)] = claims.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let attempt_id = claim
        .attempt_id
        .clone()
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    plan_suspend_step_and_plan(
        plan,
        plan.state_version,
        context,
        PlanStepSuspension {
            step_id: (*step_id).clone(),
            attempt_id,
            execution_id: execution_id.as_str().into(),
            continuation_kind: plan_continuation_kind(suspension.suspension_kind)?.into(),
            continuation_reference: suspension.suspension_id.clone(),
        },
    )
    .map_err(GoalPlanCoordinationError::Plan)
}

/// Plans one atomic Plan/Step resume around a prevalidated C6 continuation.
pub fn plan_continue_owned_plan_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    context: &PlanCommandContext,
    turn: &PlannedTurn,
) -> Result<PlannedPlanCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Suspended {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    if goal.session_version != expected_session_version {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut candidates = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= goal.snapshot.revision()
            && definition.goal_definition_digest() == digest
            && plan.snapshot.state() == PlanState::Suspended
    });
    let plan = candidates
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if candidates.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if !owned_turns(&facts, plan.snapshot.definition())?.contains(&turn.turn_id) {
        return Err(GoalPlanCoordinationError::ContinuationUnavailable);
    }
    let view = get_turn(
        ledger,
        &GetTurnQuery {
            session_id: session_id.clone(),
            turn_id: turn.turn_id.clone(),
            through_position: None,
        },
    )
    .map_err(GoalPlanCoordinationError::Runtime)?;
    if view.status != RuntimeTurnStatus::Suspended {
        return Err(GoalPlanCoordinationError::ContinuationUnavailable);
    }
    let prior_execution = view
        .execution_id
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let suspended = reconstruct_suspended_turn(
        &ledger
            .load_turn(&turn.turn_id)
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?,
    )
    .map_err(GoalPlanCoordinationError::Runtime)?;
    let goal_references = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "goal.suspended")
        .filter_map(|fact| {
            let value = serde_json::from_str::<Value>(fact.payload.as_json()).ok()?;
            (value.get("goal_id")?.as_str()? == goal_id
                && value.get("revision")?.as_u64()? == goal.snapshot.revision())
            .then(|| {
                value
                    .get("suspension_reference")?
                    .as_str()
                    .map(str::to_owned)
            })?
        })
        .collect::<Vec<_>>();
    if goal_references.as_slice() != [suspended.suspension_id.clone()] {
        return Err(GoalPlanCoordinationError::ContinuationUnavailable);
    }
    let claims = plan
        .active_claims
        .iter()
        .filter(|(_, claim)| claim.execution_id.as_deref() == Some(prior_execution.as_str()))
        .collect::<Vec<_>>();
    let [(step_id, claim)] = claims.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    plan_resume_step_execution(
        plan,
        plan.state_version,
        context,
        PlanStepContinuation {
            step_id: (*step_id).clone(),
            attempt_id: claim
                .attempt_id
                .clone()
                .ok_or(GoalPlanCoordinationError::CorruptState)?,
            prior_execution_id: prior_execution.as_str().into(),
            resolved_continuation_reference: suspended.suspension_id,
        },
        turn,
    )
    .map_err(GoalPlanCoordinationError::Plan)
}

/// Plans Goal suspension from the unique resumable Turn owned by its authoritative Plan.
///
/// The suspension identity and reason are reconstructed from the Turn terminal. Callers
/// cannot provide either value. Parallel live work fails closed because one Goal fact
/// cannot faithfully represent more than one continuation identity.
pub fn plan_suspend_goal_from_owned_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &GoalCommandContext,
) -> Result<PlannedGoalCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Active {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if goal.session_version != expected_session_version
        || plans.values().any(|plan| {
            plan.session_version != goal.session_version
                || plan.through_position != goal.through_position
        })
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut authoritative = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && plan.snapshot.state() == PlanState::Suspended
    });
    let plan = authoritative
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if authoritative.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let definition = plan.snapshot.definition();
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let turns = owned_turns(&facts, definition)?;
    let mut resumable = Vec::new();
    for turn_id in turns {
        let view = get_turn(
            ledger,
            &GetTurnQuery {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                through_position: None,
            },
        )
        .map_err(GoalPlanCoordinationError::Runtime)?;
        if view.observed_session_version != goal.session_version {
            return Err(GoalPlanCoordinationError::ConcurrentModification);
        }
        match view.status {
            RuntimeTurnStatus::Open => {
                return Err(GoalPlanCoordinationError::ResumableSuspensionUnavailable)
            }
            RuntimeTurnStatus::Suspended => {
                let snapshot = ledger
                    .load_turn(&turn_id)
                    .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
                resumable.push(
                    reconstruct_suspended_turn(&snapshot)
                        .map_err(GoalPlanCoordinationError::Runtime)?,
                );
            }
            RuntimeTurnStatus::Completed
            | RuntimeTurnStatus::Stopped
            | RuntimeTurnStatus::Failed => {}
        }
    }
    let [suspension] = resumable.as_slice() else {
        return Err(GoalPlanCoordinationError::ResumableSuspensionUnavailable);
    };
    let reason = match suspension.suspension_kind {
        RuntimeSuspensionKind::ApprovalRequired => "approval_required",
        RuntimeSuspensionKind::ExternalInputRequired => "external_input_required",
        RuntimeSuspensionKind::OperatorReconciliation => "operator_reconciliation",
        RuntimeSuspensionKind::PartialOutput
        | RuntimeSuspensionKind::ResourceUnavailable
        | RuntimeSuspensionKind::DelegationPending => {
            return Err(GoalPlanCoordinationError::ResumableSuspensionUnavailable)
        }
    };
    plan_goal_transition(
        ledger,
        session_id,
        goal_id,
        expected_goal_revision,
        context,
        GoalRuntimeTransition::Suspend {
            reason: reason.into(),
            suspension_reference: Some(suspension.suspension_id.clone()),
        },
    )
    .map_err(GoalPlanCoordinationError::Goal)
}

/// Plans Goal resume only after its exact owned Turn continuation is durable.
///
/// Runtime reads the Goal's committed suspension identity and matches it to a
/// later C6 continuation start. Neither binding is accepted from the caller.
pub fn plan_resume_goal_from_continued_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &GoalCommandContext,
) -> Result<PlannedGoalCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Suspended {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if goal.session_version != expected_session_version
        || plans.values().any(|plan| {
            plan.session_version != goal.session_version
                || plan.through_position != goal.through_position
        })
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut authoritative = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && matches!(
                plan.snapshot.state(),
                PlanState::Running | PlanState::Suspended
            )
    });
    let plan = authoritative
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if authoritative.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let suspensions = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "goal.suspended")
        .filter_map(|fact| {
            let value = serde_json::from_str::<Value>(fact.payload.as_json()).ok()?;
            (value.get("goal_id")?.as_str()? == goal_id
                && value.get("revision")?.as_u64()? == expected_goal_revision)
                .then_some((fact, value))
        })
        .collect::<Vec<_>>();
    let [(suspended_fact, suspended_payload)] = suspensions.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let suspension_reference = suspended_payload
        .get("suspension_reference")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let owned = owned_turns(&facts, plan.snapshot.definition())?;
    let mut continued = BTreeSet::new();
    for fact in facts.iter().filter(|fact| {
        fact.position > suspended_fact.position && fact.kind.as_str() == "turn.started"
    }) {
        let turn_id = fact
            .turn_id
            .as_ref()
            .ok_or(GoalPlanCoordinationError::CorruptState)?;
        if !owned.contains(turn_id) {
            continue;
        }
        let value = serde_json::from_str::<Value>(fact.payload.as_json())
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
        if value.get("kind").and_then(Value::as_str) == Some("continue")
            && value.get("prior_suspension_id").and_then(Value::as_str)
                == Some(suspension_reference)
        {
            continued.insert(turn_id.clone());
        }
    }
    if continued.len() != 1 {
        return Err(GoalPlanCoordinationError::ContinuationUnavailable);
    }
    let plan_reference = canonical_plan_reference(plan.snapshot.definition())?;
    plan_goal_transition(
        ledger,
        session_id,
        goal_id,
        expected_goal_revision,
        context,
        GoalRuntimeTransition::Activate {
            plan_reference: Some(plan_reference),
        },
    )
    .map_err(GoalPlanCoordinationError::Goal)
}

fn owned_turns(
    facts: &[DurableFact],
    definition: &PlanDefinitionV1,
) -> Result<BTreeSet<TurnId>, GoalPlanCoordinationError> {
    let mut turns = BTreeSet::new();
    for start in facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.step.started")
    {
        let value = serde_json::from_str::<Value>(start.payload.as_json())
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
        if value.get("plan_id").and_then(Value::as_str) != Some(definition.plan_id().as_str())
            || value.get("plan_revision").and_then(Value::as_u64)
                != Some(definition.plan_revision())
        {
            continue;
        }
        let execution_id = value
            .get("execution_id")
            .and_then(Value::as_str)
            .ok_or(GoalPlanCoordinationError::CorruptState)?;
        let executions = facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "execution.started")
            .filter(|fact| fact.execution_id.as_ref().map(|id| id.as_str()) == Some(execution_id))
            .collect::<Vec<_>>();
        let [execution] = executions.as_slice() else {
            return Err(GoalPlanCoordinationError::CorruptState);
        };
        turns.insert(
            execution
                .turn_id
                .clone()
                .ok_or(GoalPlanCoordinationError::CorruptState)?,
        );
    }
    Ok(turns)
}

fn plan_continuation_kind(
    kind: RuntimeSuspensionKind,
) -> Result<&'static str, GoalPlanCoordinationError> {
    match kind {
        RuntimeSuspensionKind::ApprovalRequired | RuntimeSuspensionKind::ExternalInputRequired => {
            Ok("interaction")
        }
        RuntimeSuspensionKind::OperatorReconciliation => Ok("reconciliation"),
        RuntimeSuspensionKind::PartialOutput
        | RuntimeSuspensionKind::ResourceUnavailable
        | RuntimeSuspensionKind::DelegationPending => {
            Err(GoalPlanCoordinationError::ResumableSuspensionUnavailable)
        }
    }
}

/// Plans the next missing Turn cancellation caused by a committed Goal cancellation.
///
/// Callers commit at most one result and evaluate again. This makes multi-Turn
/// propagation restart-safe without an authoritative process-local queue.
pub fn plan_next_turn_cancellation_for_goal(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    recorded_at: &str,
) -> Result<Option<PlannedGoalTurnCancellation>, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Cancelled {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let plan_coordinates = plans
        .values()
        .filter(|plan| plan.snapshot.definition().goal_id() == goal_id)
        .map(|plan| {
            (
                plan.snapshot.definition().plan_id().as_str().to_owned(),
                plan.snapshot.definition().plan_revision(),
            )
        })
        .collect::<BTreeSet<_>>();
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let cancellations = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "goal.cancelled")
        .filter_map(|fact| {
            let value = serde_json::from_str::<Value>(fact.payload.as_json()).ok()?;
            (value.get("goal_id")?.as_str()? == goal_id).then_some(fact)
        })
        .collect::<Vec<_>>();
    let [cancellation] = cancellations.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let mut turns = BTreeSet::new();
    for start in facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.step.started")
    {
        let value = serde_json::from_str::<Value>(start.payload.as_json())
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
        let plan_id = value
            .get("plan_id")
            .and_then(Value::as_str)
            .ok_or(GoalPlanCoordinationError::CorruptState)?;
        let plan_revision = value
            .get("plan_revision")
            .and_then(Value::as_u64)
            .ok_or(GoalPlanCoordinationError::CorruptState)?;
        if !plan_coordinates.contains(&(plan_id.into(), plan_revision)) {
            continue;
        }
        let execution_id = value
            .get("execution_id")
            .and_then(Value::as_str)
            .ok_or(GoalPlanCoordinationError::CorruptState)?;
        let executions = facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "execution.started")
            .filter(|fact| fact.execution_id.as_ref().map(|id| id.as_str()) == Some(execution_id))
            .collect::<Vec<_>>();
        let [execution] = executions.as_slice() else {
            return Err(GoalPlanCoordinationError::CorruptState);
        };
        turns.insert(
            execution
                .turn_id
                .clone()
                .ok_or(GoalPlanCoordinationError::CorruptState)?,
        );
    }
    for turn_id in turns {
        let view = get_turn(
            ledger,
            &GetTurnQuery {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                through_position: None,
            },
        )
        .map_err(GoalPlanCoordinationError::Runtime)?;
        if view.cancellation_requested
            || matches!(
                view.status,
                RuntimeTurnStatus::Completed
                    | RuntimeTurnStatus::Stopped
                    | RuntimeTurnStatus::Failed
            )
        {
            continue;
        }
        let command_id = RuntimeCommandId::new(format!(
            "goal-cancel:{}:{}:{}",
            goal_id,
            cancellation.fact_id.as_str(),
            turn_id.as_str()
        ))
        .map_err(GoalPlanCoordinationError::Runtime)?;
        let planned = plan_cancel_turn(&CancelTurnCommand {
            command_id,
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            reason: CancelReason::Policy,
            requested_through_position: goal.through_position,
            recorded_at: recorded_at.into(),
        })
        .map_err(GoalPlanCoordinationError::Runtime)?;
        return Ok(Some(PlannedGoalTurnCancellation {
            expected_session_version: goal.session_version,
            turn_id,
            planned,
        }));
    }
    Ok(None)
}

/// Plans Goal success from the unique completed Plan's reverified reduction evidence.
///
/// Runtime re-observes every reference at the current prefix rather than accepting
/// evidence or a terminal claim from a product client.
pub fn plan_succeed_goal_from_completed_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &GoalCommandContext,
) -> Result<PlannedGoalCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if goal.session_version != expected_session_version
        || plans.values().any(|plan| {
            plan.session_version != goal.session_version
                || plan.through_position != goal.through_position
        })
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut completed = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && plan.snapshot.state() == PlanState::Completed
    });
    let plan = completed
        .next()
        .ok_or(GoalPlanCoordinationError::CompletedPlanUnavailable)?;
    if completed.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    if watermark.session_version != expected_session_version
        || watermark.max_position != goal.through_position
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let definition = plan.snapshot.definition();
    let terminals = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.completed")
        .filter_map(|fact| {
            let value = serde_json::from_str::<Value>(fact.payload.as_json()).ok()?;
            (value.get("plan_id")?.as_str()? == definition.plan_id().as_str()
                && value.get("plan_revision")?.as_u64()? == definition.plan_revision())
            .then_some(value)
        })
        .collect::<Vec<_>>();
    let [terminal] = terminals.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let binding = terminal
        .get("reduction_evidence")
        .and_then(Value::as_object)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let inline = binding
        .get("inline_utf8")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let digest = binding
        .get("digest")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    CanonicalPayload::from_canonical_parts(inline.into(), digest.into())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let evidence = GoalEvidenceV1::list_from_canonical_json(inline)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?
        .into_iter()
        .map(|item| {
            GoalEvidenceV1::new(
                item.evidence_id().clone(),
                item.criterion_id().clone(),
                item.kind(),
                item.durable_reference(),
                item.evidence_digest(),
                expected_session_version,
            )
            .map_err(|_| GoalPlanCoordinationError::CorruptState)
        })
        .collect::<Result<Vec<_>, _>>()?;
    plan_goal_transition(
        ledger,
        session_id,
        goal_id,
        expected_goal_revision,
        context,
        GoalRuntimeTransition::Succeed { evidence },
    )
    .map_err(GoalPlanCoordinationError::Goal)
}

/// Plans Goal failure from the unique failed authoritative Plan.
///
/// The stable code is derived from `plan.failed`; callers cannot provide a
/// different failure classification or bypass the Plan terminal.
pub fn plan_fail_goal_from_failed_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &GoalCommandContext,
) -> Result<PlannedGoalCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    if goal.snapshot.state() != GoalState::Active
        || goal.session_version != expected_session_version
        || goal.snapshot.revision() != expected_goal_revision
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut failed = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && plan.snapshot.state() == PlanState::Failed
    });
    let plan = failed
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if failed.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let definition = plan.snapshot.definition();
    let terminals = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.failed")
        .filter_map(|fact| {
            let value = serde_json::from_str::<Value>(fact.payload.as_json()).ok()?;
            (value.get("plan_id")?.as_str()? == definition.plan_id().as_str()
                && value.get("plan_revision")?.as_u64()? == definition.plan_revision())
            .then_some(value)
        })
        .collect::<Vec<_>>();
    let [terminal] = terminals.as_slice() else {
        return Err(GoalPlanCoordinationError::CorruptState);
    };
    let reason = terminal
        .get("reason")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    plan_goal_transition(
        ledger,
        session_id,
        goal_id,
        expected_goal_revision,
        context,
        GoalRuntimeTransition::Fail {
            code: format!("plan_{reason}"),
            evidence: None,
        },
    )
    .map_err(GoalPlanCoordinationError::Goal)
}

/// Plans Goal activation from the unique authoritative Plan at one durable prefix.
///
/// The Plan reference is derived from verified Runtime state and is never accepted
/// from a Host body, model output, or worker request.
pub fn plan_activate_goal_from_authoritative_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    expected_session_version: u64,
    expected_goal_revision: u64,
    context: &GoalCommandContext,
) -> Result<PlannedGoalCommand, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if goal.session_version != expected_session_version
        || plans.values().any(|plan| {
            plan.session_version != goal.session_version
                || plan.through_position != goal.through_position
        })
    {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    if goal.snapshot.revision() != expected_goal_revision {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::RevisionConflict,
        ));
    }
    if goal.snapshot.state() != GoalState::Draft {
        return Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid,
        ));
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let mut authoritative = plans.values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() == expected_goal_revision
            && definition.goal_definition_digest() == goal_digest
            && matches!(
                plan.snapshot.state(),
                PlanState::Adopted | PlanState::Running | PlanState::Suspended
            )
    });
    let plan = authoritative
        .next()
        .ok_or(GoalPlanCoordinationError::AuthoritativePlanUnavailable)?;
    if authoritative.next().is_some() {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    let plan_reference = canonical_plan_reference(plan.snapshot.definition())?;
    plan_goal_transition(
        ledger,
        session_id,
        goal_id,
        expected_goal_revision,
        context,
        GoalRuntimeTransition::Activate {
            plan_reference: Some(plan_reference),
        },
    )
    .map_err(GoalPlanCoordinationError::Goal)
}

pub(crate) fn canonical_plan_reference(
    definition: &PlanDefinitionV1,
) -> Result<String, GoalPlanCoordinationError> {
    serde_jcs::to_string(&json!({
        "definition_digest": definition
            .digest()
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?,
        "plan_id": definition.plan_id().as_str(),
        "revision": definition.plan_revision(),
    }))
    .map_err(|_| GoalPlanCoordinationError::CorruptState)
}
