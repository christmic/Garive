use garive_goal::GoalEvidenceV1;
use garive_ledger::{
    CanonicalPayload, CommitResult, FactDraft, FactId, FactKind, LedgerError, SessionId,
};
use garive_plan::{PlanDefinitionV1, PlanErrorCode, PlanSnapshot, PlanStepId, PlanTransition};
use serde_json::{json, Map, Value};

use super::{
    validate_active_goal_binding, validate_goal_anchor_binding, validate_suspended_goal_binding,
    ActivePlanClaim, PlanCommandContext, PlanRuntimeError, PlanRuntimeState, PlanRuntimeTransition,
    PlanStepContinuation, PlanStepExecutionStart, PlanStepSuspension, PlannedPlanCommand,
};
use crate::{PlannedTurn, SqliteLedger, SqliteLedgerError};

struct GoalPrefix {
    facts: Vec<garive_ledger::DurableFact>,
    graph: BTreeMap<String, crate::GoalRuntimeState>,
    session_version: u64,
    through_position: u64,
}

/// Plans `plan.proposed` state version 1 without mutating durable state.
pub fn plan_propose_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    context: &PlanCommandContext,
    definition: PlanDefinitionV1,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    validate_context(context)?;
    let prefix = goal_prefix(ledger, session_id)?;
    validate_goal_anchor_binding(&definition, goal(&prefix, definition.goal_id())?)?;
    let canonical = definition.canonical_json().map_err(map_plan)?;
    let digest = definition.digest().map_err(map_plan)?;
    let mut existing = 0usize;
    let mut exact_replay = false;
    for fact in prefix
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.proposed")
    {
        let value: Value = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
        if value.get("goal_id").and_then(Value::as_str) != Some(definition.goal_id()) {
            continue;
        }
        existing = existing
            .checked_add(1)
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        if value.get("plan_id").and_then(Value::as_str) == Some(definition.plan_id().as_str())
            && value.get("plan_revision").and_then(Value::as_u64)
                == Some(definition.plan_revision())
        {
            if value.get("command_id").and_then(Value::as_str) != Some(context.command_id.as_str())
                || value.get("plan_digest").and_then(Value::as_str) != Some(digest.as_str())
            {
                return Err(PlanRuntimeError::CommandConflict);
            }
            exact_replay = true;
        }
    }
    let goal = prefix
        .graph
        .get(definition.goal_id())
        .ok_or(PlanRuntimeError::BindingStale)?;
    if !exact_replay
        && existing
            >= usize::try_from(goal.snapshot.definition().bounds().max_plan_revisions())
                .map_err(|_| PlanRuntimeError::Invalid)?
    {
        return Err(PlanRuntimeError::BoundExceeded);
    }
    let payload = json!({
        "command_id": context.command_id,
        "plan_id": definition.plan_id().as_str(),
        "plan_revision": definition.plan_revision(),
        "state_version": 1,
        "plan_digest": digest,
        "definition": {"digest": digest, "inline_utf8": canonical},
        "goal_id": definition.goal_id(),
        "goal_revision": definition.goal_revision(),
        "goal_definition_digest": definition.goal_definition_digest(),
        "agent_snapshot_digest": definition.agent_snapshot_digest(),
        "tool_catalogue_digest": definition.tool_catalogue_digest(),
        "safety_policy_revision": definition.safety_policy_revision(),
        "proposer_reference": context.actor_reference,
    });
    Ok(PlannedPlanCommand {
        facts: vec![fact(context, "plan.proposed", payload)?],
        next: PlanRuntimeState {
            snapshot: PlanSnapshot::new(definition),
            state_version: 1,
            active_claims: BTreeMap::new(),
            session_version: prefix.session_version,
            through_position: prefix.through_position,
        },
    })
}

/// Adopts one proposal only while its exact durable Goal binding is current.
pub fn plan_adopt_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    request: PlanRuntimeTransition,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    if !matches!(request, PlanRuntimeTransition::Adopt { .. }) {
        return Err(PlanRuntimeError::TransitionInvalid);
    }
    let prefix = goal_prefix(ledger, session_id)?;
    if current.session_version != prefix.session_version
        || current.through_position != prefix.through_position
    {
        return Err(PlanRuntimeError::RevisionConflict);
    }
    validate_goal_anchor_binding(
        current.snapshot.definition(),
        goal(&prefix, current.snapshot.definition().goal_id())?,
    )?;
    if let PlanRuntimeTransition::Adopt {
        expected_goal_revision,
        ..
    } = &request
    {
        if *expected_goal_revision != current.snapshot.definition().goal_revision() {
            return Err(PlanRuntimeError::BindingStale);
        }
    }
    plan_transition(current, expected_state_version, context, request)
}

/// Plans one exact-state-version normal-path transition.
pub fn plan_plan_transition(
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    request: PlanRuntimeTransition,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    if matches!(
        request,
        PlanRuntimeTransition::Start { .. }
            | PlanRuntimeTransition::Adopt { .. }
            | PlanRuntimeTransition::SuspendStep(_)
            | PlanRuntimeTransition::SuspendPlan { .. }
            | PlanRuntimeTransition::ResumePlan { .. }
            | PlanRuntimeTransition::ResumeStep { .. }
            | PlanRuntimeTransition::CompletePlan { .. }
    ) {
        return Err(PlanRuntimeError::TransitionInvalid);
    }
    plan_transition(current, expected_state_version, context, request)
}

/// Completes one Plan only after Runtime verifies complete Goal evidence at the fixed prefix.
pub fn plan_complete_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    evidence: Vec<GoalEvidenceV1>,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    let prefix = goal_prefix(ledger, session_id)?;
    if current.session_version != prefix.session_version
        || current.through_position != prefix.through_position
    {
        return Err(PlanRuntimeError::RevisionConflict);
    }
    validate_active_goal_binding(
        current.snapshot.definition(),
        goal(&prefix, current.snapshot.definition().goal_id())?,
    )?;
    let goal = prefix
        .graph
        .get(current.snapshot.definition().goal_id())
        .ok_or(PlanRuntimeError::BindingStale)?;
    crate::goal_evidence::verify_goal_success_evidence(
        current.snapshot.definition().goal_id(),
        goal.snapshot.definition().criteria(),
        &evidence,
        &prefix.graph,
        &prefix.facts,
        prefix.session_version,
        None,
    )
    .map_err(map_goal_evidence)?;
    let evidence_json =
        GoalEvidenceV1::canonical_json(&evidence).map_err(|_| PlanRuntimeError::EvidenceInvalid)?;
    let evidence_value =
        serde_json::from_str(&evidence_json).map_err(|_| PlanRuntimeError::EvidenceInvalid)?;
    let reduction_evidence = CanonicalPayload::from_value(&evidence_value)
        .map_err(|_| PlanRuntimeError::EvidenceInvalid)?;
    plan_transition(
        current,
        expected_state_version,
        context,
        PlanRuntimeTransition::CompletePlan { reduction_evidence },
    )
}

fn map_goal_evidence(error: crate::GoalRuntimeError) -> PlanRuntimeError {
    match error {
        crate::GoalRuntimeError::EvidenceInsufficient
        | crate::GoalRuntimeError::EvidenceInvalid => PlanRuntimeError::EvidenceInvalid,
        crate::GoalRuntimeError::DurabilityFailure => PlanRuntimeError::DurabilityFailure,
        _ => PlanRuntimeError::RecoveryCorrupt,
    }
}

fn goal_prefix(
    ledger: &SqliteLedger,
    session_id: &SessionId,
) -> Result<GoalPrefix, PlanRuntimeError> {
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::BindingStale)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let graph = crate::goal_recovery::reconstruct_goal_graph_from_facts(
        &facts,
        watermark.session_version,
        watermark.max_position,
    )
    .map_err(|_| PlanRuntimeError::BindingStale)?;
    Ok(GoalPrefix {
        facts,
        graph,
        session_version: watermark.session_version,
        through_position: watermark.max_position,
    })
}

fn goal<'a>(
    prefix: &'a GoalPrefix,
    goal_id: &str,
) -> Result<&'a crate::GoalRuntimeState, PlanRuntimeError> {
    prefix
        .graph
        .get(goal_id)
        .ok_or(PlanRuntimeError::BindingStale)
}

fn plan_transition(
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    request: PlanRuntimeTransition,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    validate_context(context)?;
    if current.state_version != expected_state_version {
        return Err(PlanRuntimeError::RevisionConflict);
    }
    let next_version = expected_state_version
        .checked_add(1)
        .ok_or(PlanRuntimeError::Invalid)?;
    let definition = current.snapshot.definition();
    let mut claims = current.active_claims.clone();
    let (kind, payload, transitions) = match request {
        PlanRuntimeTransition::Adopt {
            expected_goal_revision,
            expected_prior_plan_revision,
            policy_reference,
            carry_forward_evidence,
        } => {
            require_non_empty(&policy_reference)?;
            if expected_goal_revision == 0
                || expected_prior_plan_revision.is_some()
                || carry_forward_evidence.as_json() != "[]"
            {
                return Err(PlanRuntimeError::Invalid);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert(
                "expected_goal_revision".into(),
                json!(expected_goal_revision),
            );
            if let Some(revision) = expected_prior_plan_revision {
                value.insert("expected_prior_plan_revision".into(), json!(revision));
            }
            value.insert("actor_reference".into(), json!(context.actor_reference));
            value.insert("policy_reference".into(), json!(policy_reference));
            value.insert(
                "carry_forward_evidence".into(),
                content(&carry_forward_evidence),
            );
            (
                "plan.adopted",
                Value::Object(value),
                vec![PlanTransition::Adopt],
            )
        }
        PlanRuntimeTransition::Claim {
            step_id,
            claim_id,
            worker_reference,
            lease_epoch,
            clock_revision,
            claimed_at_tick,
            expires_at_tick,
        } => {
            require_non_empty(&claim_id)?;
            require_non_empty(&worker_reference)?;
            require_non_empty(&clock_revision)?;
            if lease_epoch == 0
                || expires_at_tick <= claimed_at_tick
                || claims.contains_key(&step_id)
            {
                return Err(PlanRuntimeError::ClaimStale);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(step_id.as_str()));
            value.insert(
                "step_digest".into(),
                json!(definition.step_digest(&step_id).map_err(map_plan)?),
            );
            value.insert("claim_id".into(), json!(claim_id));
            value.insert("worker_reference".into(), json!(worker_reference));
            value.insert("lease_epoch".into(), json!(lease_epoch));
            value.insert("clock_revision".into(), json!(clock_revision));
            value.insert("claimed_at_tick".into(), json!(claimed_at_tick));
            value.insert("expires_at_tick".into(), json!(expires_at_tick));
            claims.insert(
                step_id.clone(),
                ActivePlanClaim {
                    claim_id,
                    worker_reference,
                    lease_epoch,
                    clock_revision,
                    claimed_at_tick,
                    expires_at_tick,
                    attempt_id: None,
                    execution_id: None,
                },
            );
            (
                "plan.step.claimed",
                Value::Object(value),
                vec![PlanTransition::Claim(step_id)],
            )
        }
        PlanRuntimeTransition::ExpireClaim {
            step_id,
            claim_id,
            lease_epoch,
            clock_revision,
            observed_at_tick,
        } => {
            let claim = exact_claim(&claims, &step_id, &claim_id, lease_epoch, &clock_revision)?;
            if claim.attempt_id.is_some() || observed_at_tick < claim.expires_at_tick {
                return Err(PlanRuntimeError::ClaimStale);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(step_id.as_str()));
            value.insert("claim_id".into(), json!(claim_id));
            value.insert("lease_epoch".into(), json!(lease_epoch));
            value.insert("clock_revision".into(), json!(clock_revision));
            value.insert("observed_at_tick".into(), json!(observed_at_tick));
            claims.remove(&step_id);
            (
                "plan.step.claim_expired",
                Value::Object(value),
                vec![PlanTransition::ExpireClaim(step_id)],
            )
        }
        PlanRuntimeTransition::Start {
            step_id,
            claim_id,
            lease_epoch,
            clock_revision,
            observed_at_tick,
            attempt_id,
            execution_id,
            execution_snapshot_digest,
            sandbox_profile_digest,
            safety_decision_id,
        } => {
            require_non_empty(&attempt_id)?;
            require_non_empty(&execution_id)?;
            require_non_empty(&safety_decision_id)?;
            require_digest(&execution_snapshot_digest)?;
            require_digest(&sandbox_profile_digest)?;
            let claim = exact_claim(&claims, &step_id, &claim_id, lease_epoch, &clock_revision)?;
            if claim.attempt_id.is_some() || observed_at_tick >= claim.expires_at_tick {
                return Err(PlanRuntimeError::ClaimStale);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(step_id.as_str()));
            value.insert(
                "step_digest".into(),
                json!(definition.step_digest(&step_id).map_err(map_plan)?),
            );
            value.insert("claim_id".into(), json!(claim_id));
            value.insert("lease_epoch".into(), json!(lease_epoch));
            value.insert("clock_revision".into(), json!(clock_revision));
            value.insert("observed_at_tick".into(), json!(observed_at_tick));
            value.insert("attempt_id".into(), json!(attempt_id));
            value.insert("execution_id".into(), json!(execution_id));
            value.insert(
                "execution_snapshot_digest".into(),
                json!(execution_snapshot_digest),
            );
            value.insert(
                "sandbox_profile_digest".into(),
                json!(sandbox_profile_digest),
            );
            value.insert("safety_decision_id".into(), json!(safety_decision_id));
            let claim = claims
                .get_mut(&step_id)
                .ok_or(PlanRuntimeError::ClaimStale)?;
            claim.attempt_id = Some(attempt_id);
            claim.execution_id = Some(execution_id);
            (
                "plan.step.started",
                Value::Object(value),
                vec![PlanTransition::Start(step_id)],
            )
        }
        PlanRuntimeTransition::CompleteStep {
            step_id,
            attempt_id,
            execution_id,
            result_digest,
            step_evidence,
            criterion_evidence,
        } => {
            require_digest(&result_digest)?;
            let claim = claims.get(&step_id).ok_or(PlanRuntimeError::ClaimStale)?;
            if claim.attempt_id.as_deref() != Some(&attempt_id)
                || claim.execution_id.as_deref() != Some(&execution_id)
            {
                return Err(PlanRuntimeError::ClaimStale);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(step_id.as_str()));
            value.insert("attempt_id".into(), json!(attempt_id));
            value.insert("execution_id".into(), json!(execution_id));
            value.insert("result_digest".into(), json!(result_digest));
            value.insert("step_evidence".into(), content(&step_evidence));
            value.insert("criterion_evidence".into(), content(&criterion_evidence));
            claims.remove(&step_id);
            (
                "plan.step.completed",
                Value::Object(value),
                vec![PlanTransition::CompleteStep(step_id)],
            )
        }
        PlanRuntimeTransition::FailStep {
            step_id,
            attempt_id,
            execution_id,
            reason,
            evidence,
            retry_posture,
        } => {
            require_non_empty(&reason)?;
            let claim = claims.get(&step_id).ok_or(PlanRuntimeError::ClaimStale)?;
            if claim.attempt_id.as_deref() != Some(&attempt_id)
                || claim.execution_id.as_deref() != Some(&execution_id)
            {
                return Err(PlanRuntimeError::ClaimStale);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(step_id.as_str()));
            value.insert("attempt_id".into(), json!(attempt_id));
            value.insert("execution_id".into(), json!(execution_id));
            value.insert("reason".into(), json!(reason));
            value.insert("retry_posture".into(), json!(retry_posture.as_str()));
            if let Some(evidence) = evidence {
                value.insert("evidence".into(), content(&evidence));
            }
            claims.remove(&step_id);
            let mut transitions = vec![PlanTransition::FailStep(step_id.clone())];
            if retry_posture == super::PlanRetryPosture::Retry {
                transitions.push(PlanTransition::RetryStep(step_id));
            }
            ("plan.step.failed", Value::Object(value), transitions)
        }
        PlanRuntimeTransition::SuspendStep(binding) => {
            continuation(&binding.continuation_kind, &binding.continuation_reference)?;
            let claim = claims
                .get(&binding.step_id)
                .ok_or(PlanRuntimeError::ClaimStale)?;
            if claim.attempt_id.as_deref() != Some(&binding.attempt_id)
                || claim.execution_id.as_deref() != Some(&binding.execution_id)
            {
                return Err(PlanRuntimeError::ClaimStale);
            }
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(binding.step_id.as_str()));
            value.insert("attempt_id".into(), json!(binding.attempt_id));
            value.insert("execution_id".into(), json!(binding.execution_id));
            value.insert("continuation_kind".into(), json!(binding.continuation_kind));
            value.insert(
                "continuation_reference".into(),
                json!(binding.continuation_reference),
            );
            (
                "plan.step.suspended",
                Value::Object(value),
                vec![PlanTransition::SuspendStep(binding.step_id)],
            )
        }
        PlanRuntimeTransition::SuspendPlan {
            continuation_kind,
            continuation_reference,
        } => {
            continuation(&continuation_kind, &continuation_reference)?;
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("continuation_kind".into(), json!(continuation_kind));
            value.insert(
                "continuation_reference".into(),
                json!(continuation_reference),
            );
            (
                "plan.suspended",
                Value::Object(value),
                vec![PlanTransition::Suspend],
            )
        }
        PlanRuntimeTransition::ResumePlan {
            resolved_continuation_reference,
        } => {
            require_non_empty(&resolved_continuation_reference)?;
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert(
                "resolved_continuation_reference".into(),
                json!(resolved_continuation_reference),
            );
            (
                "plan.resumed",
                Value::Object(value),
                vec![PlanTransition::Resume],
            )
        }
        PlanRuntimeTransition::ResumeStep {
            continuation,
            execution_id,
        } => {
            require_non_empty(&continuation.resolved_continuation_reference)?;
            require_non_empty(&execution_id)?;
            let claim = claims
                .get_mut(&continuation.step_id)
                .ok_or(PlanRuntimeError::ClaimStale)?;
            if claim.attempt_id.as_deref() != Some(&continuation.attempt_id)
                || claim.execution_id.as_deref() != Some(&continuation.prior_execution_id)
            {
                return Err(PlanRuntimeError::ClaimStale);
            }
            claim.execution_id = Some(execution_id.clone());
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("step_id".into(), json!(continuation.step_id.as_str()));
            value.insert("attempt_id".into(), json!(continuation.attempt_id));
            value.insert(
                "prior_execution_id".into(),
                json!(continuation.prior_execution_id),
            );
            value.insert("execution_id".into(), json!(execution_id));
            value.insert(
                "resolved_continuation_reference".into(),
                json!(continuation.resolved_continuation_reference),
            );
            (
                "plan.step.resumed",
                Value::Object(value),
                vec![PlanTransition::ResumeStep(continuation.step_id)],
            )
        }
        PlanRuntimeTransition::CompletePlan { reduction_evidence } => {
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("reduction_evidence".into(), content(&reduction_evidence));
            (
                "plan.completed",
                Value::Object(value),
                vec![PlanTransition::Complete {
                    criteria_complete: true,
                }],
            )
        }
    };
    let snapshot = transitions
        .into_iter()
        .try_fold(current.snapshot.clone(), |snapshot, transition| {
            snapshot.apply(transition).map_err(map_plan)
        })?;
    Ok(PlannedPlanCommand {
        facts: vec![fact(context, kind, payload)?],
        next: PlanRuntimeState {
            snapshot,
            state_version: next_version,
            active_claims: claims,
            session_version: current.session_version,
            through_position: current.through_position,
        },
    })
}

/// Atomically binds a claimed Plan step to one already planned C6 start batch.
pub fn plan_start_step_execution(
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    request: PlanStepExecutionStart,
    turn: &PlannedTurn,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    let execution_id = turn
        .execution_id
        .as_ref()
        .ok_or(PlanRuntimeError::Invalid)?;
    let execution_facts = turn
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "execution.started")
        .collect::<Vec<_>>();
    let turn_starts = turn
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "turn.started")
        .collect::<Vec<_>>();
    if execution_facts.len() != 1
        || turn_starts.len() != 1
        || turn
            .facts
            .iter()
            .any(|fact| fact.recorded_at != context.recorded_at || fact.schema_version != 1)
    {
        return Err(PlanRuntimeError::Invalid);
    }
    let turn_start: Value = serde_json::from_str(turn_starts[0].payload.as_json())
        .map_err(|_| PlanRuntimeError::Invalid)?;
    if turn_start.get("command_id").and_then(Value::as_str) != Some(context.command_id.as_str()) {
        return Err(PlanRuntimeError::Invalid);
    }
    let execution_fact = execution_facts[0];
    if execution_fact.execution_id.as_ref() != Some(execution_id)
        || execution_fact.turn_id.as_ref() != Some(&turn.turn_id)
    {
        return Err(PlanRuntimeError::Invalid);
    }
    let payload: Value = serde_json::from_str(execution_fact.payload.as_json())
        .map_err(|_| PlanRuntimeError::Invalid)?;
    let snapshot_digest = payload
        .get("snapshot_digest")
        .and_then(Value::as_str)
        .ok_or(PlanRuntimeError::Invalid)?;
    let through_position = payload
        .get("through_position")
        .and_then(Value::as_u64)
        .ok_or(PlanRuntimeError::Invalid)?;
    if snapshot_digest != current.snapshot.definition().agent_snapshot_digest()
        || through_position != current.through_position
    {
        return Err(PlanRuntimeError::Invalid);
    }
    let mut planned = plan_transition(
        current,
        expected_state_version,
        context,
        PlanRuntimeTransition::Start {
            step_id: request.step_id,
            claim_id: request.claim_id,
            lease_epoch: request.lease_epoch,
            clock_revision: request.clock_revision,
            observed_at_tick: request.observed_at_tick,
            attempt_id: request.attempt_id,
            execution_id: execution_id.as_str().into(),
            execution_snapshot_digest: snapshot_digest.into(),
            sandbox_profile_digest: request.sandbox_profile_digest,
            safety_decision_id: request.safety_decision_id,
        },
    )?;
    let mut facts = turn.facts.clone();
    facts.append(&mut planned.facts);
    planned.facts = facts;
    Ok(planned)
}

/// Atomically suspends one running step and its Plan around one C6 continuation.
pub(crate) fn plan_suspend_step_and_plan(
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    binding: PlanStepSuspension,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    let step_context = derived_context(context, "step-suspended");
    let mut step = plan_transition(
        current,
        expected_state_version,
        &step_context,
        PlanRuntimeTransition::SuspendStep(binding.clone()),
    )?;
    let plan_context = derived_context(context, "plan-suspended");
    let mut plan = plan_transition(
        &step.next,
        step.next.state_version,
        &plan_context,
        PlanRuntimeTransition::SuspendPlan {
            continuation_kind: binding.continuation_kind,
            continuation_reference: binding.continuation_reference,
        },
    )?;
    step.facts.append(&mut plan.facts);
    plan.facts = step.facts;
    Ok(plan)
}

/// Atomically resumes Plan/step state around one already planned C6 continuation.
pub(crate) fn plan_resume_step_execution(
    current: &PlanRuntimeState,
    expected_state_version: u64,
    context: &PlanCommandContext,
    continuation: PlanStepContinuation,
    turn: &PlannedTurn,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    let execution_id = turn
        .execution_id
        .as_ref()
        .ok_or(PlanRuntimeError::Invalid)?;
    let starts = turn
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "turn.started")
        .collect::<Vec<_>>();
    let executions = turn
        .facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "execution.started")
        .collect::<Vec<_>>();
    let [start] = starts.as_slice() else {
        return Err(PlanRuntimeError::Invalid);
    };
    let [execution] = executions.as_slice() else {
        return Err(PlanRuntimeError::Invalid);
    };
    let start_payload: Value =
        serde_json::from_str(start.payload.as_json()).map_err(|_| PlanRuntimeError::Invalid)?;
    if start_payload.get("command_id").and_then(Value::as_str) != Some(context.command_id.as_str())
        || start_payload.get("kind").and_then(Value::as_str) != Some("continue")
        || start_payload
            .get("prior_suspension_id")
            .and_then(Value::as_str)
            != Some(continuation.resolved_continuation_reference.as_str())
        || execution.execution_id.as_ref() != Some(execution_id)
        || turn
            .facts
            .iter()
            .any(|fact| fact.recorded_at != context.recorded_at || fact.schema_version != 1)
    {
        return Err(PlanRuntimeError::Invalid);
    }
    let plan_context = derived_context(context, "plan-resumed");
    let mut plan = plan_transition(
        current,
        expected_state_version,
        &plan_context,
        PlanRuntimeTransition::ResumePlan {
            resolved_continuation_reference: continuation.resolved_continuation_reference.clone(),
        },
    )?;
    let mut step = plan_transition(
        &plan.next,
        plan.next.state_version,
        context,
        PlanRuntimeTransition::ResumeStep {
            continuation,
            execution_id: execution_id.as_str().into(),
        },
    )?;
    plan.facts.extend(turn.facts.clone());
    plan.facts.append(&mut step.facts);
    step.facts = plan.facts;
    Ok(step)
}

/// Commits one validated Plan command under Session optimistic concurrency.
pub fn commit_plan_command(
    ledger: &mut SqliteLedger,
    session_id: SessionId,
    expected_session_version: u64,
    planned: &PlannedPlanCommand,
) -> Result<CommitResult, PlanRuntimeError> {
    let watermark = ledger
        .session_watermark(&session_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::BindingStale)?;
    let existing = ledger
        .read_facts(&session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    if planned
        .facts
        .iter()
        .any(|draft| existing.iter().any(|fact| fact.fact_id == draft.fact_id))
    {
        return ledger
            .commit(session_id, expected_session_version, planned.facts.clone())
            .map_err(map_ledger);
    }
    let binds_anchor = planned
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "plan.adopted");
    let resumes_suspended_goal = planned
        .facts
        .iter()
        .any(|fact| matches!(fact.kind.as_str(), "plan.resumed" | "plan.step.resumed"));
    let requires_active_goal = planned.facts.iter().any(|fact| {
        matches!(
            fact.kind.as_str(),
            "plan.suspended" | "plan.step.suspended" | "plan.step.claimed" | "plan.step.started"
        )
    });
    if binds_anchor || requires_active_goal || resumes_suspended_goal {
        if expected_session_version != watermark.session_version
            || planned.next.session_version != watermark.session_version
            || planned.next.through_position != watermark.max_position
        {
            return Err(PlanRuntimeError::RevisionConflict);
        }
        let prefix = goal_prefix(ledger, &session_id)?;
        let definition = planned.next.snapshot.definition();
        let goal = goal(&prefix, definition.goal_id())?;
        if binds_anchor {
            validate_goal_anchor_binding(definition, goal)?;
        } else if resumes_suspended_goal {
            validate_suspended_goal_binding(definition, goal)?;
        } else {
            validate_active_goal_binding(definition, goal)?;
        }
    }
    ledger
        .commit(session_id, expected_session_version, planned.facts.clone())
        .map_err(map_ledger)
}

fn mutation(
    context: &PlanCommandContext,
    definition: &PlanDefinitionV1,
    previous: u64,
    next: u64,
) -> Map<String, Value> {
    Map::from_iter([
        ("command_id".into(), json!(context.command_id)),
        ("plan_id".into(), json!(definition.plan_id().as_str())),
        ("plan_revision".into(), json!(definition.plan_revision())),
        ("previous_state_version".into(), json!(previous)),
        ("state_version".into(), json!(next)),
    ])
}

fn exact_claim<'a>(
    claims: &'a BTreeMap<PlanStepId, ActivePlanClaim>,
    step_id: &PlanStepId,
    claim_id: &str,
    lease_epoch: u64,
    clock_revision: &str,
) -> Result<&'a ActivePlanClaim, PlanRuntimeError> {
    claims
        .get(step_id)
        .filter(|claim| claim.claim_id == claim_id)
        .filter(|claim| claim.lease_epoch == lease_epoch)
        .filter(|claim| claim.clock_revision == clock_revision)
        .ok_or(PlanRuntimeError::ClaimStale)
}

fn content(payload: &CanonicalPayload) -> Value {
    json!({"digest": payload.sha256(), "inline_utf8": payload.as_json()})
}

fn fact(
    context: &PlanCommandContext,
    kind: &str,
    payload: Value,
) -> Result<FactDraft, PlanRuntimeError> {
    Ok(FactDraft {
        fact_id: FactId::try_from(context.command_id.as_str())
            .map_err(|_| PlanRuntimeError::Invalid)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| PlanRuntimeError::Invalid)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).map_err(|_| PlanRuntimeError::Invalid)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn validate_context(context: &PlanCommandContext) -> Result<(), PlanRuntimeError> {
    require_non_empty(&context.command_id)?;
    require_non_empty(&context.actor_reference)?;
    chrono::DateTime::parse_from_rfc3339(&context.recorded_at)
        .map(|_| ())
        .map_err(|_| PlanRuntimeError::Invalid)
}

fn require_non_empty(value: &str) -> Result<(), PlanRuntimeError> {
    if value.is_empty() {
        Err(PlanRuntimeError::Invalid)
    } else {
        Ok(())
    }
}

fn require_digest(value: &str) -> Result<(), PlanRuntimeError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(PlanRuntimeError::Invalid)
    }
}

fn continuation(kind: &str, reference: &str) -> Result<(), PlanRuntimeError> {
    if !matches!(kind, "interaction" | "reconciliation") {
        return Err(PlanRuntimeError::Invalid);
    }
    require_non_empty(reference)
}

fn derived_context(context: &PlanCommandContext, suffix: &str) -> PlanCommandContext {
    PlanCommandContext {
        command_id: format!("{}:{suffix}", context.command_id),
        actor_reference: context.actor_reference.clone(),
        recorded_at: context.recorded_at.clone(),
    }
}

fn map_plan(error: garive_plan::PlanError) -> PlanRuntimeError {
    match error.code() {
        PlanErrorCode::PlanInvalid => PlanRuntimeError::Invalid,
        PlanErrorCode::PlanCycle | PlanErrorCode::PlanTransitionInvalid => {
            PlanRuntimeError::TransitionInvalid
        }
        PlanErrorCode::StepNotReady => PlanRuntimeError::StepNotReady,
        PlanErrorCode::PlanBoundExceeded => PlanRuntimeError::BoundExceeded,
    }
}

fn map_ledger(error: SqliteLedgerError) -> PlanRuntimeError {
    match error {
        SqliteLedgerError::Domain(
            LedgerError::IdempotencyCollision | LedgerError::IncompleteReplay,
        ) => PlanRuntimeError::CommandConflict,
        SqliteLedgerError::Domain(LedgerError::ConcurrentModification) => {
            PlanRuntimeError::RevisionConflict
        }
        SqliteLedgerError::CorruptLedger(_)
        | SqliteLedgerError::UnsupportedSchema(_)
        | SqliteLedgerError::InvalidStoredValue(_) => PlanRuntimeError::RecoveryCorrupt,
        SqliteLedgerError::Domain(_) => PlanRuntimeError::Invalid,
        _ => PlanRuntimeError::DurabilityFailure,
    }
}
use std::collections::BTreeMap;
