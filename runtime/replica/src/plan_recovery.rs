use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{DurableFact, SessionId};
use garive_plan::{
    PlanCapabilityReference, PlanDefinitionV1, PlanSnapshot, PlanStepId, PlanTransition,
};
use serde_json::{Map, Value};

use crate::{ActivePlanClaim, PlanRuntimeError, PlanRuntimeState, SqliteLedger, SqliteLedgerError};

/// Reconstructs one exact Plan revision from a verified fixed Session prefix.
pub fn reconstruct_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    plan_id: &str,
    plan_revision: u64,
    required_goal_criteria: &BTreeSet<String>,
    already_satisfied_criteria: &BTreeSet<String>,
    available_capabilities: &BTreeSet<PlanCapabilityReference>,
) -> Result<PlanRuntimeState, PlanRuntimeError> {
    if plan_id.is_empty() || plan_revision == 0 {
        return Err(PlanRuntimeError::Invalid);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut state = None;
    for fact in facts
        .iter()
        .filter(|fact| belongs(fact, plan_id, plan_revision))
    {
        apply(
            &mut state,
            fact,
            required_goal_criteria,
            already_satisfied_criteria,
            available_capabilities,
        )?;
    }
    let mut state = state.ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    state.session_version = watermark.session_version;
    state.through_position = watermark.max_position;
    Ok(state)
}

fn belongs(fact: &DurableFact, plan_id: &str, revision: u64) -> bool {
    if !fact.kind.as_str().starts_with("plan.") {
        return false;
    }
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| {
            Some((
                value.get("plan_id")?.as_str()?.to_owned(),
                value.get("plan_revision")?.as_u64()?,
            ))
        })
        .is_some_and(|value| value == (plan_id.to_owned(), revision))
}

fn apply(
    state: &mut Option<PlanRuntimeState>,
    fact: &DurableFact,
    required: &BTreeSet<String>,
    satisfied: &BTreeSet<String>,
    capabilities: &BTreeSet<PlanCapabilityReference>,
) -> Result<(), PlanRuntimeError> {
    let payload: Value = serde_json::from_str(fact.payload.as_json()).map_err(corrupt)?;
    let value = payload
        .as_object()
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if fact.kind.as_str() == "plan.proposed" {
        if state.is_some() || unsigned(value, "state_version")? != 1 {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let definition = definition(value, required, satisfied, capabilities)?;
        *state = Some(PlanRuntimeState {
            snapshot: PlanSnapshot::new(definition),
            state_version: 1,
            active_claims: BTreeMap::new(),
            session_version: 0,
            through_position: 0,
        });
        return Ok(());
    }
    let current = state.as_mut().ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let previous = unsigned(value, "previous_state_version")?;
    let next = unsigned(value, "state_version")?;
    if previous != current.state_version || next != previous.checked_add(1).ok_or(corrupt(()))? {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let transitions = transitions(current, fact.kind.as_str(), value)?;
    current.snapshot = transitions
        .into_iter()
        .try_fold(current.snapshot.clone(), |snapshot, transition| {
            snapshot.apply(transition).map_err(corrupt)
        })?;
    current.state_version = next;
    Ok(())
}

fn transitions(
    current: &mut PlanRuntimeState,
    kind: &str,
    value: &Map<String, Value>,
) -> Result<Vec<PlanTransition>, PlanRuntimeError> {
    match kind {
        "plan.adopted" => {
            if unsigned(value, "expected_goal_revision")?
                != current.snapshot.definition().goal_revision()
            {
                return Err(PlanRuntimeError::RecoveryCorrupt);
            }
            Ok(vec![PlanTransition::Adopt])
        }
        "plan.rejected" => Ok(vec![PlanTransition::Reject]),
        "plan.superseded" => Ok(vec![PlanTransition::Supersede]),
        "plan.suspended" => Ok(vec![PlanTransition::Suspend]),
        "plan.resumed" => Ok(vec![PlanTransition::Resume]),
        "plan.completed" => Ok(vec![PlanTransition::Complete {
            criteria_complete: true,
        }]),
        "plan.failed" => Ok(vec![PlanTransition::Fail]),
        "plan.step.claimed" => claim(current, value).map(|value| vec![value]),
        "plan.step.claim_expired" => expire_claim(current, value).map(|value| vec![value]),
        "plan.step.started" => start(current, value).map(|value| vec![value]),
        "plan.step.completed" => {
            terminal_step(current, value, StepTerminal::Complete).map(|value| vec![value])
        }
        "plan.step.failed" => {
            let failed = terminal_step(current, value, StepTerminal::Fail)?;
            let mut result = vec![failed];
            if text(value, "retry_posture")? == "retry" {
                result.push(PlanTransition::RetryStep(step_id(value)?));
            }
            Ok(result)
        }
        "plan.step.suspended" => {
            terminal_step(current, value, StepTerminal::Suspend).map(|value| vec![value])
        }
        "plan.step.resumed" => Ok(vec![PlanTransition::ResumeStep(step_id(value)?)]),
        _ => Err(PlanRuntimeError::RecoveryCorrupt),
    }
}

fn claim(
    current: &mut PlanRuntimeState,
    value: &Map<String, Value>,
) -> Result<PlanTransition, PlanRuntimeError> {
    let step_id = step_id(value)?;
    if text(value, "step_digest")?
        != current
            .snapshot
            .definition()
            .step_digest(&step_id)
            .map_err(corrupt)?
        || current.active_claims.contains_key(&step_id)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let claim = ActivePlanClaim {
        claim_id: text(value, "claim_id")?.into(),
        worker_reference: text(value, "worker_reference")?.into(),
        lease_epoch: unsigned(value, "lease_epoch")?,
        clock_revision: text(value, "clock_revision")?.into(),
        claimed_at_tick: unsigned(value, "claimed_at_tick")?,
        expires_at_tick: unsigned(value, "expires_at_tick")?,
        attempt_id: None,
        execution_id: None,
    };
    current.active_claims.insert(step_id.clone(), claim);
    Ok(PlanTransition::Claim(step_id))
}

fn expire_claim(
    current: &mut PlanRuntimeState,
    value: &Map<String, Value>,
) -> Result<PlanTransition, PlanRuntimeError> {
    let step_id = step_id(value)?;
    let claim = exact_claim(current, &step_id, value)?;
    if claim.attempt_id.is_some()
        || claim.clock_revision != text(value, "clock_revision")?
        || unsigned(value, "observed_at_tick")? < claim.expires_at_tick
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    current.active_claims.remove(&step_id);
    Ok(PlanTransition::ExpireClaim(step_id))
}

fn start(
    current: &mut PlanRuntimeState,
    value: &Map<String, Value>,
) -> Result<PlanTransition, PlanRuntimeError> {
    let step_id = step_id(value)?;
    let claim = exact_claim(current, &step_id, value)?;
    if claim.attempt_id.is_some()
        || claim.clock_revision != text(value, "clock_revision")?
        || unsigned(value, "observed_at_tick")? >= claim.expires_at_tick
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let claim = current
        .active_claims
        .get_mut(&step_id)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    claim.attempt_id = Some(text(value, "attempt_id")?.into());
    claim.execution_id = Some(text(value, "execution_id")?.into());
    Ok(PlanTransition::Start(step_id))
}

enum StepTerminal {
    Complete,
    Fail,
    Suspend,
}

fn terminal_step(
    current: &mut PlanRuntimeState,
    value: &Map<String, Value>,
    terminal: StepTerminal,
) -> Result<PlanTransition, PlanRuntimeError> {
    let step_id = step_id(value)?;
    let claim = current
        .active_claims
        .get(&step_id)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if claim.attempt_id.as_deref() != Some(text(value, "attempt_id")?)
        || claim.execution_id.as_deref() != Some(text(value, "execution_id")?)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    current.active_claims.remove(&step_id);
    Ok(match terminal {
        StepTerminal::Complete => PlanTransition::CompleteStep(step_id),
        StepTerminal::Suspend => PlanTransition::SuspendStep(step_id),
        StepTerminal::Fail => PlanTransition::FailStep(step_id),
    })
}

fn exact_claim<'a>(
    current: &'a PlanRuntimeState,
    step_id: &PlanStepId,
    value: &Map<String, Value>,
) -> Result<&'a ActivePlanClaim, PlanRuntimeError> {
    current
        .active_claims
        .get(step_id)
        .filter(|claim| claim.claim_id == text(value, "claim_id").unwrap_or_default())
        .filter(|claim| claim.lease_epoch == unsigned(value, "lease_epoch").unwrap_or_default())
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn definition(
    value: &Map<String, Value>,
    required: &BTreeSet<String>,
    satisfied: &BTreeSet<String>,
    capabilities: &BTreeSet<PlanCapabilityReference>,
) -> Result<PlanDefinitionV1, PlanRuntimeError> {
    let json = inline(value, "definition")?;
    let definition = PlanDefinitionV1::from_canonical_json(json, required, satisfied, capabilities)
        .map_err(corrupt)?;
    if definition.digest().map_err(corrupt)? != text(value, "plan_digest")?
        || definition.goal_id() != text(value, "goal_id")?
        || definition.goal_revision() != unsigned(value, "goal_revision")?
        || definition.goal_definition_digest() != text(value, "goal_definition_digest")?
        || definition.agent_snapshot_digest() != text(value, "agent_snapshot_digest")?
        || definition.tool_catalogue_digest() != text(value, "tool_catalogue_digest")?
        || definition.safety_policy_revision() != text(value, "safety_policy_revision")?
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    Ok(definition)
}

fn step_id(value: &Map<String, Value>) -> Result<PlanStepId, PlanRuntimeError> {
    PlanStepId::new(text(value, "step_id")?).map_err(corrupt)
}

fn inline<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, PlanRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("inline_utf8"))
        .and_then(Value::as_str)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, PlanRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, PlanRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn corrupt<T>(_: T) -> PlanRuntimeError {
    PlanRuntimeError::RecoveryCorrupt
}

fn map_ledger(error: SqliteLedgerError) -> PlanRuntimeError {
    match error {
        SqliteLedgerError::Storage(_) => PlanRuntimeError::DurabilityFailure,
        _ => PlanRuntimeError::RecoveryCorrupt,
    }
}
