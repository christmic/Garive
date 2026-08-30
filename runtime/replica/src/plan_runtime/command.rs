use garive_ledger::{
    CanonicalPayload, CommitResult, FactDraft, FactId, FactKind, LedgerError, SessionId,
};
use garive_plan::{PlanDefinitionV1, PlanErrorCode, PlanSnapshot, PlanStepId, PlanTransition};
use serde_json::{json, Map, Value};

use super::{
    ActivePlanClaim, PlanCommandContext, PlanRuntimeError, PlanRuntimeState, PlanRuntimeTransition,
    PlannedPlanCommand,
};
use crate::{SqliteLedger, SqliteLedgerError};

/// Plans `plan.proposed` state version 1 without mutating durable state.
pub fn plan_propose_plan(
    context: &PlanCommandContext,
    definition: PlanDefinitionV1,
) -> Result<PlannedPlanCommand, PlanRuntimeError> {
    validate_context(context)?;
    let canonical = definition.canonical_json().map_err(map_plan)?;
    let digest = definition.digest().map_err(map_plan)?;
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
            session_version: 0,
            through_position: 0,
        },
    })
}

/// Plans one exact-state-version normal-path transition.
pub fn plan_plan_transition(
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
    let (kind, payload, transition) = match request {
        PlanRuntimeTransition::Adopt {
            expected_goal_revision,
            expected_prior_plan_revision,
            policy_reference,
            carry_forward_evidence,
        } => {
            require_non_empty(&policy_reference)?;
            if expected_goal_revision == 0 || expected_prior_plan_revision == Some(0) {
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
            ("plan.adopted", Value::Object(value), PlanTransition::Adopt)
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
                PlanTransition::Claim(step_id),
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
                PlanTransition::ExpireClaim(step_id),
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
                PlanTransition::Start(step_id),
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
                PlanTransition::CompleteStep(step_id),
            )
        }
        PlanRuntimeTransition::CompletePlan { reduction_evidence } => {
            let mut value = mutation(context, definition, expected_state_version, next_version);
            value.insert("reduction_evidence".into(), content(&reduction_evidence));
            (
                "plan.completed",
                Value::Object(value),
                PlanTransition::Complete {
                    criteria_complete: true,
                },
            )
        }
    };
    let snapshot = current.snapshot.apply(transition).map_err(map_plan)?;
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

/// Commits one validated Plan command under Session optimistic concurrency.
pub fn commit_plan_command(
    ledger: &mut SqliteLedger,
    session_id: SessionId,
    expected_session_version: u64,
    planned: &PlannedPlanCommand,
) -> Result<CommitResult, PlanRuntimeError> {
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
