//! Restart reconstruction for one committed deterministic effect batch.

use garive_ledger::{CanonicalPayload, DurableFact, ExecutionId, TurnId};
use garive_tools::EffectBatchStep;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{AuthorizedBatchInvocation, BatchRuntimeError, SqliteLedger};

/// Durable recovery state for one plan member in model order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectBatchMemberRecovery {
    /// Prepared and Authorized are durable; no Started fact exists.
    Authorized,
    /// Started exists without a trustworthy fully published terminal.
    NeedsReconciliation,
    /// A terminal exists but its ordered observation is absent.
    TerminalPendingObservation,
    /// Terminal and model-visible observation are both durable.
    Observed,
}

/// Exact committed plan and member states reconstructed without replanning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredEffectBatch {
    /// Digest revalidated from the committed canonical plan preimage.
    pub plan_digest: String,
    /// Exact committed conflict graph digest.
    pub conflict_graph_digest: String,
    /// Exact persisted steps, not a newly computed plan.
    pub steps: Vec<EffectBatchStep>,
    /// Exact committed concurrency bound.
    pub max_parallel_reads: usize,
    /// Exact committed result-buffer bound.
    pub max_buffered_result_bytes: u64,
    /// One recovery state for every supplied invocation in model order.
    pub members: Vec<EffectBatchMemberRecovery>,
}

/// Reconstructs and validates one exact committed plan from SQLite after restart.
pub fn reconstruct_effect_batch_recovery(
    ledger: &SqliteLedger,
    turn_id: &TurnId,
    execution_id: &ExecutionId,
    plan_digest: &str,
    invocations: &[AuthorizedBatchInvocation],
) -> Result<RecoveredEffectBatch, BatchRuntimeError> {
    let snapshot = ledger
        .load_turn(turn_id)
        .map_err(|_| BatchRuntimeError::DurabilityFailure)?;
    let plans: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| {
            fact.execution_id.as_ref() == Some(execution_id)
                && fact.kind.as_str() == "execution.effect_batch_planned"
                && payload(fact).is_ok_and(|value| value["plan_digest"] == plan_digest)
        })
        .collect();
    let [plan_fact] = plans.as_slice() else {
        return Err(BatchRuntimeError::InvalidBinding);
    };
    let value = payload(plan_fact)?;
    let digests = content_value(&value, "ordered_prepared_digests")?
        .as_array()
        .cloned()
        .ok_or(BatchRuntimeError::InvalidBinding)?;
    let ordered: Vec<_> = digests
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(BatchRuntimeError::InvalidBinding)
        })
        .collect::<Result<_, _>>()?;
    if ordered.len() != invocations.len()
        || invocations
            .iter()
            .zip(&ordered)
            .any(|(invocation, digest)| invocation.prepared.input_digest() != digest)
    {
        return Err(BatchRuntimeError::InvalidBinding);
    }
    let steps_value = content_value(&value, "steps")?;
    let steps: Vec<EffectBatchStep> = serde_json::from_value(steps_value.clone())
        .map_err(|_| BatchRuntimeError::InvalidBinding)?;
    let conflict_graph_digest = text(&value, "conflict_graph_digest")?.to_owned();
    let preimage = json!({
        "schema_version":1,
        "prepared_contract_version":2,
        "ordered_prepared_digests":ordered,
        "conflict_graph_digest":conflict_graph_digest,
        "steps":steps,
    });
    let canonical = serde_jcs::to_vec(&preimage).map_err(|_| BatchRuntimeError::InvalidBinding)?;
    if format!("{:x}", Sha256::digest(canonical)) != plan_digest {
        return Err(BatchRuntimeError::InvalidBinding);
    }
    validate_step_coverage(&steps, invocations.len())?;
    let members = invocations
        .iter()
        .map(|invocation| member_state(&snapshot.facts, execution_id, invocation))
        .collect::<Result<_, _>>()?;
    Ok(RecoveredEffectBatch {
        plan_digest: plan_digest.to_owned(),
        conflict_graph_digest,
        steps,
        max_parallel_reads: unsigned(&value, "max_parallel_reads")?
            .try_into()
            .map_err(|_| BatchRuntimeError::InvalidBinding)?,
        max_buffered_result_bytes: unsigned(&value, "max_buffered_result_bytes")?,
        members,
    })
}

fn member_state(
    facts: &[DurableFact],
    execution_id: &ExecutionId,
    invocation: &AuthorizedBatchInvocation,
) -> Result<EffectBatchMemberRecovery, BatchRuntimeError> {
    let relevant: Vec<_> = facts
        .iter()
        .filter(|fact| {
            fact.execution_id.as_ref() == Some(execution_id)
                && fact
                    .tool_invocation_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == invocation.invocation_id.as_str())
        })
        .collect();
    let prepared = relevant.iter().any(|fact| {
        fact.kind.as_str() == "effect.prepared"
            && fact.schema_version == 2
            && payload(fact)
                .is_ok_and(|value| value["prepared_digest"] == invocation.prepared.input_digest())
    });
    let authorized = relevant.iter().any(|fact| {
        fact.kind.as_str() == "effect.authorized"
            && payload(fact).is_ok_and(|value| {
                value["prepared_digest"] == invocation.prepared.input_digest()
                    && value["grant_id"] == invocation.grant.grant_id.as_str()
            })
    });
    if !prepared || !authorized {
        return Err(BatchRuntimeError::InvalidBinding);
    }
    let has = |kind: &str| relevant.iter().any(|fact| fact.kind.as_str() == kind);
    let started = has("effect.started");
    let terminal = has("effect.completed") || has("effect.failed") || has("effect.reconciled");
    let uncertain = has("effect.uncertain");
    let observation = has("effect.observation");
    match (started, terminal, uncertain, observation) {
        (_, true, _, true) => Ok(EffectBatchMemberRecovery::Observed),
        (_, true, _, false) => Ok(EffectBatchMemberRecovery::TerminalPendingObservation),
        (true, false, _, false) => Ok(EffectBatchMemberRecovery::NeedsReconciliation),
        (false, false, false, false) => Ok(EffectBatchMemberRecovery::Authorized),
        _ => Err(BatchRuntimeError::InvalidBinding),
    }
}

fn validate_step_coverage(
    steps: &[EffectBatchStep],
    member_count: usize,
) -> Result<(), BatchRuntimeError> {
    let indexes: Vec<_> = steps
        .iter()
        .flat_map(|step| match step {
            EffectBatchStep::SequentialStep { intent_index } => vec![*intent_index],
            EffectBatchStep::ParallelReadGroup { intent_indexes } => intent_indexes.clone(),
        })
        .collect();
    if indexes == (0..member_count).collect::<Vec<_>>() {
        Ok(())
    } else {
        Err(BatchRuntimeError::InvalidBinding)
    }
}

fn payload(fact: &DurableFact) -> Result<Map<String, Value>, BatchRuntimeError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(BatchRuntimeError::InvalidBinding)
}

fn content_value(value: &Map<String, Value>, key: &str) -> Result<Value, BatchRuntimeError> {
    let binding = value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(BatchRuntimeError::InvalidBinding)?;
    let canonical = CanonicalPayload::from_canonical_parts(
        text(binding, "inline_utf8")?.to_owned(),
        text(binding, "digest")?.to_owned(),
    )
    .map_err(|_| BatchRuntimeError::InvalidBinding)?;
    serde_json::from_str(canonical.as_json()).map_err(|_| BatchRuntimeError::InvalidBinding)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, BatchRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(BatchRuntimeError::InvalidBinding)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, BatchRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(BatchRuntimeError::InvalidBinding)
}
