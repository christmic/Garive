//! Commit-version-bound PL1 carry-forward evidence planning.

use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{
    CanonicalPayload, CommitResult, DurableFact, FactDraft, FactId, FactKind, LedgerError,
    SessionId,
};
use garive_plan::{PlanState, PlanStepId, PlanTransition, StepState};
use serde_json::{json, Map, Value};

use crate::{
    PlanCommandContext, PlanRuntimeError, PlanRuntimeState, SqliteLedger, SqliteLedgerError,
};

/// Runtime-verified completed-step evidence for one replacement adoption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPlanCarryForward {
    source_plan_id: String,
    source_plan_revision: u64,
    source_session_version: u64,
    target_plan_revision: u64,
    target_plan_digest: String,
    carried_steps: BTreeSet<PlanStepId>,
    evidence: CanonicalPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CarryForwardRecord {
    pub step_id: PlanStepId,
    pub step_digest: String,
    pub result_digest: String,
    pub dependency_results: BTreeMap<PlanStepId, String>,
    pub step_evidence_digest: String,
    pub criterion_evidence_digest: String,
    pub terminal_fact_id: String,
    pub terminal_position: u64,
    pub terminal_commit_version: u64,
}

impl VerifiedPlanCarryForward {
    /// Returns completed steps admitted into the target revision.
    pub const fn carried_steps(&self) -> &BTreeSet<PlanStepId> {
        &self.carried_steps
    }

    /// Returns the canonical evidence document stored by `plan.adopted`.
    pub const fn evidence(&self) -> &CanonicalPayload {
        &self.evidence
    }

    pub(crate) fn matches_source(&self, source: &PlanRuntimeState) -> bool {
        self.source_plan_id == source.snapshot.definition().plan_id().as_str()
            && self.source_plan_revision == source.snapshot.definition().plan_revision()
            && self.source_session_version == source.session_version
    }

    fn matches_target(&self, target: &PlanRuntimeState) -> bool {
        self.target_plan_revision == target.snapshot.definition().plan_revision()
            && target
                .snapshot
                .definition()
                .digest()
                .is_ok_and(|digest| digest == self.target_plan_digest)
    }
}

/// Atomic old/new revision mutation and both predicted projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedPlanReplacement {
    /// Ordered source Superseded then target Adopted facts.
    pub facts: Vec<FactDraft>,
    /// Source revision after Supersede.
    pub source_next: PlanRuntimeState,
    /// Target revision after verified carry-forward adoption.
    pub target_next: PlanRuntimeState,
}

/// Derives the maximal safe dependency-closed carry set at one SQLite watermark.
pub fn verify_plan_carry_forward(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    source: &PlanRuntimeState,
    target: &PlanRuntimeState,
) -> Result<VerifiedPlanCarryForward, PlanRuntimeError> {
    let old = source.snapshot.definition();
    let new = target.snapshot.definition();
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::BindingStale)?;
    if !matches!(
        source.snapshot.state(),
        PlanState::Adopted | PlanState::Running | PlanState::Suspended
    ) || target.snapshot.state() != PlanState::Proposed
        || source.session_version != target.session_version
        || source.through_position != target.through_position
        || source.session_version != watermark.session_version
        || source.through_position != watermark.max_position
        || old.plan_id() != new.plan_id()
        || old.plan_revision().checked_add(1) != Some(new.plan_revision())
        || old.goal_id() != new.goal_id()
        || old.goal_revision() != new.goal_revision()
        || old.goal_definition_digest() != new.goal_definition_digest()
    {
        return Err(PlanRuntimeError::RevisionConflict);
    }
    let facts = ledger
        .read_facts(session_id, 0, source.through_position, None)
        .map_err(map_ledger)?;
    let goals = crate::goal_recovery::reconstruct_goal_graph_from_facts(
        &facts,
        watermark.session_version,
        watermark.max_position,
    )
    .map_err(|_| PlanRuntimeError::BindingStale)?;
    let goal = goals
        .get(new.goal_id())
        .ok_or(PlanRuntimeError::BindingStale)?;
    if goal.snapshot.state().is_terminal()
        || goal.snapshot.revision() != new.goal_revision()
        || goal
            .snapshot
            .definition()
            .digest()
            .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?
            != new.goal_definition_digest()
    {
        return Err(PlanRuntimeError::BindingStale);
    }
    let completed = completion_facts(&facts, old.plan_id().as_str(), old.plan_revision())?;
    let mut carried = BTreeSet::new();
    let mut records = Vec::new();
    for step in new.steps() {
        let id = step.step_id();
        if source.snapshot.step(id).map(|value| value.state()) != Some(StepState::Completed)
            || !step.depends_on().is_subset(&carried)
            || old.step_digest(id).map_err(|_| PlanRuntimeError::Invalid)?
                != new.step_digest(id).map_err(|_| PlanRuntimeError::Invalid)?
        {
            continue;
        }
        let terminal = completed.get(id).ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        let value = payload(terminal)?;
        let commit_version = ledger
            .fact_commit_version(&terminal.fact_id)
            .map_err(map_ledger)?
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        if commit_version > source.session_version || terminal.position > source.through_position {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let dependencies = step
            .depends_on()
            .iter()
            .map(|dependency| {
                let fact = completed
                    .get(dependency)
                    .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
                let value = payload(fact)?;
                Ok(json!({
                    "step_id":dependency.as_str(),
                    "result_digest":text(&value,"result_digest")?,
                }))
            })
            .collect::<Result<Vec<_>, PlanRuntimeError>>()?;
        records.push(json!({
            "step_id":id.as_str(),
            "step_digest":new.step_digest(id).map_err(|_| PlanRuntimeError::Invalid)?,
            "result_digest":text(&value,"result_digest")?,
            "dependency_results":dependencies,
            "step_evidence_digest":content_digest(&value,"step_evidence")?,
            "criterion_evidence_digest":content_digest(&value,"criterion_evidence")?,
            "terminal_fact_id":terminal.fact_id.as_str(),
            "terminal_position":terminal.position,
            "terminal_commit_version":commit_version,
        }));
        carried.insert(id.clone());
    }
    let evidence = CanonicalPayload::from_value(&Value::Array(records))
        .map_err(|_| PlanRuntimeError::Invalid)?;
    Ok(VerifiedPlanCarryForward {
        source_plan_id: old.plan_id().as_str().into(),
        source_plan_revision: old.plan_revision(),
        source_session_version: source.session_version,
        target_plan_revision: new.plan_revision(),
        target_plan_digest: new.digest().map_err(|_| PlanRuntimeError::Invalid)?,
        carried_steps: carried,
        evidence,
    })
}

/// Plans one indivisible source Supersede and target verified adoption.
pub fn plan_plan_replacement(
    source: &PlanRuntimeState,
    target: &PlanRuntimeState,
    verified: &VerifiedPlanCarryForward,
    context: &PlanCommandContext,
    policy_reference: &str,
) -> Result<PlannedPlanReplacement, PlanRuntimeError> {
    if context.command_id.is_empty()
        || context.actor_reference.is_empty()
        || policy_reference.is_empty()
        || chrono::DateTime::parse_from_rfc3339(&context.recorded_at).is_err()
        || source.session_version != target.session_version
        || source.through_position != target.through_position
        || !verified.matches_source(source)
        || !verified.matches_target(target)
    {
        return Err(PlanRuntimeError::RevisionConflict);
    }
    let old = source.snapshot.definition();
    let new = target.snapshot.definition();
    let source_version = source
        .state_version
        .checked_add(1)
        .ok_or(PlanRuntimeError::Invalid)?;
    let target_version = target
        .state_version
        .checked_add(1)
        .ok_or(PlanRuntimeError::Invalid)?;
    let unresolved = source
        .snapshot
        .definition()
        .steps()
        .iter()
        .filter_map(|step| {
            let progress = source.snapshot.step(step.step_id())?;
            (progress.state() != StepState::Completed).then(
                || json!({"step_id":step.step_id().as_str(),"state":step_state(progress.state())}),
            )
        })
        .collect::<Vec<_>>();
    let source_payload = json!({
        "command_id":context.command_id,
        "plan_id":old.plan_id().as_str(),"plan_revision":old.plan_revision(),
        "previous_state_version":source.state_version,"state_version":source_version,
        "replacement_plan_id":new.plan_id().as_str(),
        "replacement_plan_revision":new.plan_revision(),
        "replacement_plan_digest":verified.target_plan_digest,
        "unresolved_work":content_value(&Value::Array(unresolved))?,
    });
    let target_payload = json!({
        "command_id":context.command_id,
        "plan_id":new.plan_id().as_str(),"plan_revision":new.plan_revision(),
        "previous_state_version":target.state_version,"state_version":target_version,
        "expected_goal_revision":new.goal_revision(),
        "expected_prior_plan_revision":old.plan_revision(),
        "actor_reference":context.actor_reference,"policy_reference":policy_reference,
        "carry_forward_evidence":{"digest":verified.evidence.sha256(),"inline_utf8":verified.evidence.as_json()},
    });
    let source_snapshot = source
        .snapshot
        .apply(PlanTransition::Supersede)
        .map_err(|_| PlanRuntimeError::TransitionInvalid)?;
    let target_snapshot = target
        .snapshot
        .apply(PlanTransition::AdoptWithCarryForward(
            verified.carried_steps.clone(),
        ))
        .map_err(|_| PlanRuntimeError::TransitionInvalid)?;
    Ok(PlannedPlanReplacement {
        facts: vec![
            plan_fact(context, "source", "plan.superseded", source_payload)?,
            plan_fact(context, "target", "plan.adopted", target_payload)?,
        ],
        source_next: next_state(source, source_snapshot, source_version),
        target_next: next_state(target, target_snapshot, target_version),
    })
}

/// Commits the two-revision mutation at the evidence watermark.
pub fn commit_plan_replacement(
    ledger: &mut SqliteLedger,
    session_id: SessionId,
    expected_session_version: u64,
    planned: &PlannedPlanReplacement,
) -> Result<CommitResult, PlanRuntimeError> {
    ledger
        .commit(session_id, expected_session_version, planned.facts.clone())
        .map_err(map_ledger)
}

fn next_state(
    current: &PlanRuntimeState,
    snapshot: garive_plan::PlanSnapshot,
    state_version: u64,
) -> PlanRuntimeState {
    PlanRuntimeState {
        snapshot,
        state_version,
        active_claims: current.active_claims.clone(),
        session_version: current.session_version,
        through_position: current.through_position,
    }
}

fn plan_fact(
    context: &PlanCommandContext,
    suffix: &str,
    kind: &str,
    payload: Value,
) -> Result<FactDraft, PlanRuntimeError> {
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("{}-{suffix}", context.command_id).as_str())
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

fn content_value(value: &Value) -> Result<Value, PlanRuntimeError> {
    let payload = CanonicalPayload::from_value(value).map_err(|_| PlanRuntimeError::Invalid)?;
    Ok(json!({"digest":payload.sha256(),"inline_utf8":payload.as_json()}))
}

const fn step_state(value: StepState) -> &'static str {
    match value {
        StepState::Pending => "pending",
        StepState::Ready => "ready",
        StepState::Claimed => "claimed",
        StepState::Running => "running",
        StepState::Suspended => "suspended",
        StepState::Completed => "completed",
        StepState::Failed => "failed",
    }
}

fn completion_facts<'a>(
    facts: &'a [DurableFact],
    plan_id: &str,
    revision: u64,
) -> Result<BTreeMap<PlanStepId, &'a DurableFact>, PlanRuntimeError> {
    let mut output = BTreeMap::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.step.completed")
    {
        let value = payload(fact)?;
        if text(&value, "plan_id")? != plan_id
            || value.get("plan_revision").and_then(Value::as_u64) != Some(revision)
        {
            continue;
        }
        let id = PlanStepId::new(text(&value, "step_id")?)
            .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
        if output.insert(id, fact).is_some() {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
    }
    Ok(output)
}

pub(crate) fn decode_carried_steps(
    evidence: &str,
) -> Result<BTreeSet<PlanStepId>, PlanRuntimeError> {
    Ok(decode_carry_forward_records(evidence)?
        .into_iter()
        .map(|record| record.step_id)
        .collect())
}

pub(crate) fn decode_carry_forward_records(
    evidence: &str,
) -> Result<Vec<CarryForwardRecord>, PlanRuntimeError> {
    let records = serde_json::from_str::<Value>(evidence)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let mut carried = BTreeSet::new();
    let mut decoded = Vec::new();
    for record in records {
        let value = record
            .as_object()
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        let expected = BTreeSet::from([
            "criterion_evidence_digest",
            "dependency_results",
            "result_digest",
            "step_digest",
            "step_evidence_digest",
            "step_id",
            "terminal_commit_version",
            "terminal_fact_id",
            "terminal_position",
        ]);
        if value.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected
            || ![
                "criterion_evidence_digest",
                "result_digest",
                "step_digest",
                "step_evidence_digest",
            ]
            .iter()
            .all(|key| valid_digest(text(value, key).unwrap_or_default()))
            || value
                .get("terminal_commit_version")
                .and_then(Value::as_u64)
                .is_none_or(|number| number == 0)
            || value
                .get("terminal_position")
                .and_then(Value::as_u64)
                .is_none_or(|number| number == 0)
        {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let dependencies = value
            .get("dependency_results")
            .and_then(Value::as_array)
            .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
        if dependencies.iter().any(|dependency| {
            dependency.as_object().is_none_or(|entry| {
                entry.keys().map(String::as_str).collect::<BTreeSet<_>>()
                    != BTreeSet::from(["result_digest", "step_id"])
                    || !valid_digest(
                        entry
                            .get("result_digest")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
                    || entry
                        .get("step_id")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
            })
        }) {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let id = PlanStepId::new(text(value, "step_id")?)
            .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
        if text(value, "terminal_fact_id")?.is_empty() || !carried.insert(id.clone()) {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        let dependency_results = dependencies
            .iter()
            .map(|dependency| {
                let dependency = dependency
                    .as_object()
                    .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
                Ok((
                    PlanStepId::new(text(dependency, "step_id")?)
                        .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?,
                    text(dependency, "result_digest")?.to_owned(),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, PlanRuntimeError>>()?;
        if dependency_results.len() != dependencies.len() {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        }
        decoded.push(CarryForwardRecord {
            step_id: id,
            step_digest: text(value, "step_digest")?.into(),
            result_digest: text(value, "result_digest")?.into(),
            dependency_results,
            step_evidence_digest: text(value, "step_evidence_digest")?.into(),
            criterion_evidence_digest: text(value, "criterion_evidence_digest")?.into(),
            terminal_fact_id: text(value, "terminal_fact_id")?.into(),
            terminal_position: value["terminal_position"]
                .as_u64()
                .ok_or(PlanRuntimeError::RecoveryCorrupt)?,
            terminal_commit_version: value["terminal_commit_version"]
                .as_u64()
                .ok_or(PlanRuntimeError::RecoveryCorrupt)?,
        });
    }
    Ok(decoded)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
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
        .filter(|value| !value.is_empty())
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
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
