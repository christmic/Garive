use std::collections::{BTreeMap, BTreeSet};

use garive_goal::GoalEvidenceV1;
use garive_ledger::{CanonicalPayload, DurableFact, SessionId};
use garive_plan::{
    PlanCapabilityReference, PlanDefinitionV1, PlanSnapshot, PlanStepId, PlanTransition,
};
use serde_json::{Map, Value};

use crate::plan_carry_forward::{decode_carried_steps, decode_carry_forward_records};
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
    reconstruct_plan_from_facts(
        ledger,
        &facts,
        watermark.session_version,
        watermark.max_position,
        plan_id,
        plan_revision,
        required_goal_criteria,
        already_satisfied_criteria,
        available_capabilities,
    )
}

/// Reconstructs every Plan revision from one verified fixed Session prefix.
pub fn reconstruct_plan_graph(
    ledger: &SqliteLedger,
    session_id: &SessionId,
) -> Result<BTreeMap<(String, u64), PlanRuntimeState>, PlanRuntimeError> {
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut graph = BTreeMap::new();
    for proposal in facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.proposed")
    {
        let value = payload(proposal)?;
        let plan_id = text(&value, "plan_id")?.to_owned();
        let plan_revision = unsigned(&value, "plan_revision")?;
        if graph.contains_key(&(plan_id.clone(), plan_revision)) {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let proposal_version = ledger
            .fact_commit_version(&proposal.fact_id)
            .map_err(map_ledger)?
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        let goal_facts = facts
            .iter()
            .filter(|fact| fact.position <= proposal.position)
            .cloned()
            .collect::<Vec<_>>();
        let goals = crate::goal_recovery::reconstruct_goal_graph_from_facts(
            &goal_facts,
            proposal_version,
            proposal.position,
        )
        .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
        let goal = goals
            .get(text(&value, "goal_id")?)
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        let definition = goal.snapshot.definition();
        if goal.snapshot.state().is_terminal()
            || goal.snapshot.revision() != unsigned(&value, "goal_revision")?
            || definition.digest().map_err(corrupt)? != text(&value, "goal_definition_digest")?
        {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let required = definition
            .criteria()
            .iter()
            .map(|criterion| criterion.criterion_id().as_str().to_owned())
            .collect::<BTreeSet<_>>();
        let capabilities = definition
            .capability_references()
            .iter()
            .map(|reference| {
                PlanCapabilityReference::new(reference.name(), reference.exact_revision())
                    .map_err(corrupt)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let state = reconstruct_plan_from_facts(
            ledger,
            &facts,
            watermark.session_version,
            watermark.max_position,
            &plan_id,
            plan_revision,
            &required,
            &BTreeSet::new(),
            &capabilities,
        )?;
        graph.insert((plan_id, plan_revision), state);
    }
    if facts.iter().any(|fact| {
        fact.kind.as_str().starts_with("plan.")
            && !matches!(
                fact.kind.as_str(),
                "plan.proposal.requested" | "plan.proposal.result_bound" | "plan.replan.admitted"
            )
            && plan_coordinates(fact)
                .ok()
                .is_none_or(|coordinates| !graph.contains_key(&coordinates))
    }) {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    Ok(graph)
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_plan_from_facts(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    session_version: u64,
    through_position: u64,
    plan_id: &str,
    plan_revision: u64,
    required_goal_criteria: &BTreeSet<String>,
    already_satisfied_criteria: &BTreeSet<String>,
    available_capabilities: &BTreeSet<PlanCapabilityReference>,
) -> Result<PlanRuntimeState, PlanRuntimeError> {
    let mut state = None;
    for fact in facts
        .iter()
        .filter(|fact| belongs(fact, plan_id, plan_revision))
    {
        apply(
            &mut state,
            ledger,
            facts,
            fact,
            required_goal_criteria,
            already_satisfied_criteria,
            available_capabilities,
        )?;
    }
    let mut state = state.ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    state.session_version = session_version;
    state.through_position = through_position;
    Ok(state)
}

fn plan_coordinates(fact: &DurableFact) -> Result<(String, u64), PlanRuntimeError> {
    let value = payload(fact)?;
    Ok((
        text(&value, "plan_id")?.to_owned(),
        unsigned(&value, "plan_revision")?,
    ))
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
    ledger: &SqliteLedger,
    facts: &[DurableFact],
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
    if matches!(
        fact.kind.as_str(),
        "plan.step.started" | "plan.step.resumed"
    ) {
        validate_execution_binding(ledger, facts, fact, value, current)?;
    }
    if matches!(fact.kind.as_str(), "plan.resumed" | "plan.step.resumed") {
        validate_continuation_resolution(facts, fact, value)?;
    }
    if fact.kind.as_str() == "plan.completed" {
        validate_completion_evidence(ledger, facts, fact, value, current)?;
    }
    if fact.kind.as_str() == "plan.superseded"
        || (fact.kind.as_str() == "plan.adopted"
            && value.get("expected_prior_plan_revision").is_some())
    {
        validate_replacement_binding(
            ledger,
            facts,
            fact,
            value,
            current,
            required,
            satisfied,
            capabilities,
        )?;
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

fn validate_continuation_resolution(
    facts: &[DurableFact],
    resumed: &DurableFact,
    value: &Map<String, Value>,
) -> Result<(), PlanRuntimeError> {
    let plan_id = text(value, "plan_id")?;
    let plan_revision = unsigned(value, "plan_revision")?;
    let step_id = value.get("step_id").and_then(Value::as_str);
    let suspended_kind = if step_id.is_some() {
        "plan.step.suspended"
    } else {
        "plan.suspended"
    };
    let candidates = facts
        .iter()
        .filter(|fact| fact.position < resumed.position && fact.kind.as_str() == suspended_kind)
        .filter_map(|fact| {
            let candidate = payload(fact).ok()?;
            (candidate.get("plan_id")?.as_str()? == plan_id
                && candidate.get("plan_revision")?.as_u64()? == plan_revision
                && candidate.get("step_id").and_then(Value::as_str) == step_id)
                .then_some(candidate)
        })
        .collect::<Vec<_>>();
    let suspended = candidates.last().ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if text(suspended, "continuation_reference")? != text(value, "resolved_continuation_reference")?
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    Ok(())
}

fn validate_completion_evidence(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    completion: &DurableFact,
    value: &Map<String, Value>,
    current: &PlanRuntimeState,
) -> Result<(), PlanRuntimeError> {
    let commit_version = ledger
        .fact_commit_version(&completion.fact_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let observed_version = commit_version
        .checked_sub(1)
        .filter(|version| *version > 0)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let mut prior_facts = Vec::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.position < completion.position)
    {
        if ledger
            .fact_commit_version(&fact.fact_id)
            .map_err(map_ledger)?
            .is_some_and(|version| version <= observed_version)
        {
            prior_facts.push(fact.clone());
        }
    }
    let through_position = prior_facts.last().map_or(0, |fact| fact.position);
    let graph = crate::goal_recovery::reconstruct_goal_graph_from_facts(
        &prior_facts,
        observed_version,
        through_position,
    )
    .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
    let definition = current.snapshot.definition();
    let goal = graph
        .get(definition.goal_id())
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    crate::plan_runtime::validate_active_goal_binding(definition, goal)
        .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
    let evidence =
        GoalEvidenceV1::list_from_canonical_json(bound_inline(value, "reduction_evidence")?)
            .map_err(corrupt)?;
    crate::goal_evidence::verify_goal_success_evidence(
        definition.goal_id(),
        goal.snapshot.definition().criteria(),
        &evidence,
        &graph,
        &prior_facts,
        observed_version,
        None,
    )
    .map_err(|_| PlanRuntimeError::RecoveryCorrupt)
}

fn validate_execution_binding(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    plan_fact: &DurableFact,
    value: &Map<String, Value>,
    current: &PlanRuntimeState,
) -> Result<(), PlanRuntimeError> {
    let execution_id = text(value, "execution_id")?;
    let snapshot_digest = if plan_fact.kind.as_str() == "plan.step.started" {
        text(value, "execution_snapshot_digest")?
    } else {
        current.snapshot.definition().agent_snapshot_digest()
    };
    if snapshot_digest != current.snapshot.definition().agent_snapshot_digest() {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let commit_version = ledger
        .fact_commit_version(&plan_fact.fact_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let executions = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "execution.started")
        .filter(|fact| fact.execution_id.as_ref().map(|id| id.as_str()) == Some(execution_id))
        .collect::<Vec<_>>();
    if executions.len() != 1
        || ledger
            .fact_commit_version(&executions[0].fact_id)
            .map_err(map_ledger)?
            != Some(commit_version)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let execution = executions[0];
    let turn_id = execution
        .turn_id
        .as_ref()
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let execution_payload: Value =
        serde_json::from_str(execution.payload.as_json()).map_err(corrupt)?;
    if execution_payload
        .get("snapshot_digest")
        .and_then(Value::as_str)
        != Some(snapshot_digest)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let command_id = text(value, "command_id")?;
    let turn_starts = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "turn.started")
        .filter(|fact| fact.turn_id.as_ref() == Some(turn_id))
        .filter(|fact| {
            ledger.fact_commit_version(&fact.fact_id).ok().flatten() == Some(commit_version)
        })
        .collect::<Vec<_>>();
    if turn_starts.len() != 1 {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let turn_payload: Value =
        serde_json::from_str(turn_starts[0].payload.as_json()).map_err(corrupt)?;
    if turn_payload.get("command_id").and_then(Value::as_str) != Some(command_id)
        || (plan_fact.kind.as_str() == "plan.step.resumed"
            && (turn_payload.get("kind").and_then(Value::as_str) != Some("continue")
                || turn_payload
                    .get("prior_suspension_id")
                    .and_then(Value::as_str)
                    != Some(text(value, "resolved_continuation_reference")?)))
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
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
            let evidence = inline(value, "carry_forward_evidence")?;
            let carried = decode_carried_steps(evidence)?;
            if value.get("expected_prior_plan_revision").is_none() && !carried.is_empty() {
                return Err(PlanRuntimeError::RecoveryCorrupt);
            }
            Ok(vec![if carried.is_empty() {
                PlanTransition::Adopt
            } else {
                PlanTransition::AdoptWithCarryForward(carried)
            }])
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
        "plan.step.resumed" => resume_step(current, value).map(|value| vec![value]),
        _ => Err(PlanRuntimeError::RecoveryCorrupt),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_replacement_binding(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    fact: &DurableFact,
    value: &Map<String, Value>,
    current: &PlanRuntimeState,
    required: &BTreeSet<String>,
    satisfied: &BTreeSet<String>,
    capabilities: &BTreeSet<PlanCapabilityReference>,
) -> Result<(), PlanRuntimeError> {
    let command = text(value, "command_id")?;
    let commit_version = ledger
        .fact_commit_version(&fact.fact_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let counterpart_kind = if fact.kind.as_str() == "plan.superseded" {
        "plan.adopted"
    } else {
        "plan.superseded"
    };
    let counterparts = facts
        .iter()
        .filter(|candidate| candidate.kind.as_str() == counterpart_kind)
        .filter_map(|candidate| {
            let payload = serde_json::from_str::<Value>(candidate.payload.as_json()).ok()?;
            let payload = payload.as_object()?;
            (payload.get("command_id").and_then(Value::as_str) == Some(command))
                .then_some((candidate, payload.clone()))
        })
        .collect::<Vec<_>>();
    let [(counterpart, other)] = counterparts.as_slice() else {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    };
    if ledger
        .fact_commit_version(&counterpart.fact_id)
        .map_err(map_ledger)?
        != Some(commit_version)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let (source, target) = if fact.kind.as_str() == "plan.superseded" {
        (value, other)
    } else {
        (other, value)
    };
    if text(source, "replacement_plan_id")? != text(target, "plan_id")?
        || unsigned(source, "replacement_plan_revision")? != unsigned(target, "plan_revision")?
        || unsigned(target, "expected_prior_plan_revision")? != unsigned(source, "plan_revision")?
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let proposals = facts
        .iter()
        .filter(|candidate| candidate.kind.as_str() == "plan.proposed")
        .filter_map(|candidate| {
            let payload = serde_json::from_str::<Value>(candidate.payload.as_json()).ok()?;
            let payload = payload.as_object()?;
            (payload.get("plan_id").and_then(Value::as_str) == target.get("plan_id")?.as_str()
                && payload.get("plan_revision").and_then(Value::as_u64)
                    == target.get("plan_revision")?.as_u64())
            .then_some(payload.clone())
        })
        .collect::<Vec<_>>();
    let [proposal] = proposals.as_slice() else {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    };
    if text(source, "replacement_plan_digest")? != text(proposal, "plan_digest")? {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    if fact.kind.as_str() == "plan.adopted" {
        validate_carry_evidence(
            ledger,
            facts,
            target,
            source,
            current,
            commit_version,
            required,
            satisfied,
            capabilities,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_carry_evidence(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    target: &Map<String, Value>,
    source: &Map<String, Value>,
    current: &PlanRuntimeState,
    replacement_version: u64,
    required: &BTreeSet<String>,
    satisfied: &BTreeSet<String>,
    capabilities: &BTreeSet<PlanCapabilityReference>,
) -> Result<(), PlanRuntimeError> {
    let records = decode_carry_forward_records(bound_inline(target, "carry_forward_evidence")?)?;
    let source_id = text(source, "plan_id")?;
    let source_revision = unsigned(source, "plan_revision")?;
    let source_proposals = facts
        .iter()
        .filter(|candidate| candidate.kind.as_str() == "plan.proposed")
        .filter_map(|candidate| {
            let payload = serde_json::from_str::<Value>(candidate.payload.as_json()).ok()?;
            let payload = payload.as_object()?;
            (payload.get("plan_id")?.as_str()? == source_id
                && payload.get("plan_revision")?.as_u64()? == source_revision)
                .then_some(payload.clone())
        })
        .collect::<Vec<_>>();
    let [source_proposal] = source_proposals.as_slice() else {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    };
    let old = definition(source_proposal, required, satisfied, capabilities)?;
    let new = current.snapshot.definition();
    if old.goal_id() != new.goal_id()
        || old.goal_revision() > new.goal_revision()
        || old.goal_definition_digest() != new.goal_definition_digest()
        || records
            .iter()
            .map(|record| &record.step_id)
            .collect::<Vec<_>>()
            != new
                .steps()
                .iter()
                .filter(|step| {
                    records
                        .iter()
                        .any(|record| &record.step_id == step.step_id())
                })
                .map(|step| step.step_id())
                .collect::<Vec<_>>()
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    for record in records {
        let step = new
            .steps()
            .iter()
            .find(|step| step.step_id() == &record.step_id)
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        if new.step_digest(&record.step_id).map_err(corrupt)? != record.step_digest
            || old.step_digest(&record.step_id).map_err(corrupt)? != record.step_digest
            || step.depends_on()
                != &record
                    .dependency_results
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>()
        {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        validate_terminal_record(
            ledger,
            facts,
            source_id,
            source_revision,
            &record,
            replacement_version,
        )?;
    }
    Ok(())
}

fn validate_terminal_record(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    source_id: &str,
    source_revision: u64,
    record: &crate::plan_carry_forward::CarryForwardRecord,
    replacement_version: u64,
) -> Result<(), PlanRuntimeError> {
    let terminals = facts
        .iter()
        .filter(|candidate| candidate.fact_id.as_str() == record.terminal_fact_id)
        .collect::<Vec<_>>();
    let [terminal] = terminals.as_slice() else {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    };
    let value = serde_json::from_str::<Value>(terminal.payload.as_json())
        .map_err(corrupt)?
        .as_object()
        .cloned()
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if terminal.kind.as_str() != "plan.step.completed"
        || terminal.position != record.terminal_position
        || ledger
            .fact_commit_version(&terminal.fact_id)
            .map_err(map_ledger)?
            != Some(record.terminal_commit_version)
        || record.terminal_commit_version > replacement_version
        || text(&value, "plan_id")? != source_id
        || unsigned(&value, "plan_revision")? != source_revision
        || text(&value, "step_id")? != record.step_id.as_str()
        || text(&value, "result_digest")? != record.result_digest
        || content_digest(&value, "step_evidence")? != record.step_evidence_digest
        || content_digest(&value, "criterion_evidence")? != record.criterion_evidence_digest
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    for (dependency, result_digest) in &record.dependency_results {
        let matches = facts
            .iter()
            .filter(|candidate| candidate.kind.as_str() == "plan.step.completed")
            .filter_map(|candidate| {
                let value = serde_json::from_str::<Value>(candidate.payload.as_json()).ok()?;
                let value = value.as_object()?.clone();
                (text(&value, "plan_id").ok() == Some(source_id)
                    && unsigned(&value, "plan_revision").ok() == Some(source_revision)
                    && text(&value, "step_id").ok() == Some(dependency.as_str()))
                .then_some((candidate, value))
            })
            .collect::<Vec<_>>();
        let [(dependency_fact, dependency_value)] = matches.as_slice() else {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        };
        if text(dependency_value, "result_digest")? != result_digest
            || ledger
                .fact_commit_version(&dependency_fact.fact_id)
                .map_err(map_ledger)?
                .is_none_or(|version| version > replacement_version)
        {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
    }
    Ok(())
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

#[derive(Clone, Copy)]
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
    if !matches!(terminal, StepTerminal::Suspend) {
        current.active_claims.remove(&step_id);
    }
    Ok(match terminal {
        StepTerminal::Complete => PlanTransition::CompleteStep(step_id),
        StepTerminal::Suspend => PlanTransition::SuspendStep(step_id),
        StepTerminal::Fail => PlanTransition::FailStep(step_id),
    })
}

fn resume_step(
    current: &mut PlanRuntimeState,
    value: &Map<String, Value>,
) -> Result<PlanTransition, PlanRuntimeError> {
    let step_id = step_id(value)?;
    let claim = current
        .active_claims
        .get_mut(&step_id)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if claim.attempt_id.as_deref() != Some(text(value, "attempt_id")?)
        || claim.execution_id.as_deref() != Some(text(value, "prior_execution_id")?)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    claim.execution_id = Some(text(value, "execution_id")?.into());
    Ok(PlanTransition::ResumeStep(step_id))
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

fn bound_inline<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, PlanRuntimeError> {
    let binding = value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if binding.keys().map(String::as_str).collect::<BTreeSet<_>>()
        != BTreeSet::from(["digest", "inline_utf8"])
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let json = text(binding, "inline_utf8")?;
    CanonicalPayload::from_canonical_parts(json.to_owned(), text(binding, "digest")?.to_owned())
        .map_err(corrupt)?;
    Ok(json)
}

fn content_digest<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, PlanRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
        .and_then(|binding| text(binding, "digest"))
}

fn payload(fact: &DurableFact) -> Result<Map<String, Value>, PlanRuntimeError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.as_object().cloned())
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
