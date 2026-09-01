use garive_goal::GoalEvidenceV1;
use garive_ledger::{CanonicalPayload, SessionId};
use garive_plan::{PlanDefinitionV1, PlanState};
use serde_json::{json, Value};

use crate::{
    plan_goal_transition, reconstruct_goal, reconstruct_plan_graph, GoalCommandContext,
    GoalRuntimeError, GoalRuntimeTransition, PlannedGoalCommand, SqliteLedger,
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
    /// Goal planning rejected the derived activation.
    Goal(GoalRuntimeError),
    /// Plan recovery or canonical reference derivation failed closed.
    CorruptState,
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
            && definition.goal_revision() == expected_goal_revision
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
