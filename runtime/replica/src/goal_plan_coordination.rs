use std::collections::BTreeSet;

use garive_goal::{GoalEvidenceV1, GoalState};
use garive_ledger::{CanonicalPayload, DurableFact, SessionId, TurnId};
use garive_plan::{PlanDefinitionV1, PlanState};
use serde_json::{json, Value};

use crate::{
    get_turn, plan_cancel_turn, plan_goal_transition, reconstruct_goal, reconstruct_plan_graph,
    reconstruct_suspended_turn, CancelReason, CancelTurnCommand, GetTurnQuery, GoalCommandContext,
    GoalRuntimeError, GoalRuntimeTransition, PlannedGoalCommand, PlannedTurn, RuntimeCommandError,
    RuntimeCommandId, RuntimeSuspensionKind, RuntimeTurnStatus, SqliteLedger,
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
    /// Goal planning rejected the derived activation.
    Goal(GoalRuntimeError),
    /// Durable Turn planning rejected a derived cancellation.
    Runtime(RuntimeCommandError),
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
