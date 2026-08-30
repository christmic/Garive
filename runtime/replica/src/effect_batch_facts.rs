//! Durable admission facts for one deterministic C5b plan.

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, ToolInvocationId as LedgerToolId,
    TurnId,
};
use garive_tools::{EffectBatchPlanV1, ReplayClass};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{AuthorizedBatchInvocation, BatchRuntimeError};

/// Frozen ownership, observation time, and committed plan bounds.
#[derive(Clone, Debug)]
pub struct EffectBatchAdmissionContext {
    /// Turn owning the already-active Execution.
    pub turn_id: TurnId,
    /// Disposable active Execution owning the batch.
    pub execution_id: ExecutionId,
    /// Exact non-zero concurrency bound committed with the plan.
    pub max_parallel_reads: usize,
    /// Exact non-zero aggregate result-buffer charge.
    pub max_buffered_result_bytes: u64,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Ordered atomic admission facts ending with `execution.effect_batch_planned.v1`.
#[derive(Clone, Debug)]
pub struct PlannedEffectBatchAdmission {
    /// Exact plan digest bound by the final fact.
    pub plan_digest: String,
    /// Prepared v2, Authorized, then plan facts in deterministic model order.
    pub facts: Vec<FactDraft>,
}

/// Builds the exact additive C5b admission transaction without performing I/O.
pub fn plan_effect_batch_admission(
    context: &EffectBatchAdmissionContext,
    plan: &EffectBatchPlanV1,
    invocations: &[AuthorizedBatchInvocation],
) -> Result<PlannedEffectBatchAdmission, BatchRuntimeError> {
    chrono::DateTime::parse_from_rfc3339(&context.recorded_at)
        .map_err(|_| BatchRuntimeError::InvalidBinding)?;
    if context.max_parallel_reads == 0
        || context.max_buffered_result_bytes == 0
        || invocations.len() != plan.ordered_prepared_digests().len()
    {
        return Err(BatchRuntimeError::InvalidBinding);
    }
    let mut facts = Vec::with_capacity(invocations.len() * 2 + 1);
    for (invocation, digest) in invocations.iter().zip(plan.ordered_prepared_digests()) {
        validate_invocation(invocation, digest)?;
        let prepared = &invocation.prepared;
        facts.push(fact(
            context,
            &child_id(plan.plan_digest(), invocation.invocation_id.as_str(), "prepared"),
            "effect.prepared",
            2,
            Some(invocation.invocation_id.as_str()),
            json!({
                "prepared_contract_version":2,
                "prepared_digest":prepared.input_digest(),
                "tool_name":prepared.tool_name(),
                "tool_revision":prepared.tool_revision(),
                "replay_class":replay_class(prepared.replay_class()),
                "model_call_id":prepared.model_call_id(),
                "access_policy_revision":prepared.access_policy_revision().ok_or(BatchRuntimeError::InvalidBinding)?,
                "access_resolver_revision":prepared.access_resolver_revision().ok_or(BatchRuntimeError::InvalidBinding)?,
                "invocation_accesses":content(&serde_json::to_value(prepared.invocation_accesses().ok_or(BatchRuntimeError::InvalidBinding)?).map_err(|_| BatchRuntimeError::InvalidBinding)?)?,
                "max_result_bytes":prepared.max_result_bytes().ok_or(BatchRuntimeError::InvalidBinding)?,
            }),
        )?);
        facts.push(fact(
            context,
            &child_id(plan.plan_digest(), invocation.invocation_id.as_str(), "authorized"),
            "effect.authorized",
            1,
            Some(invocation.invocation_id.as_str()),
            json!({
                "prepared_digest":prepared.input_digest(),
                "grant_id":invocation.grant.grant_id.as_str(),
                "authority_revision":invocation.grant.authority_revision,
                "granted_requirements":content(&serde_json::to_value(&invocation.grant.granted_requirements).map_err(|_| BatchRuntimeError::InvalidBinding)?)?,
            }),
        )?);
    }
    facts.push(fact(
        context,
        &format!("fact-{}", sha256(format!("{}:plan", plan.plan_digest()).as_bytes())),
        "execution.effect_batch_planned",
        1,
        None,
        json!({
            "plan_digest":plan.plan_digest(),
            "conflict_graph_digest":plan.conflict_graph_digest(),
            "ordered_prepared_digests":content(&json!(plan.ordered_prepared_digests()))?,
            "steps":content(&serde_json::to_value(plan.steps()).map_err(|_| BatchRuntimeError::InvalidBinding)?)?,
            "max_parallel_reads":context.max_parallel_reads,
            "max_buffered_result_bytes":context.max_buffered_result_bytes,
        }),
    )?);
    Ok(PlannedEffectBatchAdmission {
        plan_digest: plan.plan_digest().to_owned(),
        facts,
    })
}

fn validate_invocation(
    invocation: &AuthorizedBatchInvocation,
    digest: &str,
) -> Result<(), BatchRuntimeError> {
    if invocation.prepared.contract_version() != 2
        || invocation.prepared.input_digest() != digest
        || invocation.grant.invocation_id != invocation.invocation_id
        || invocation.grant.prepared_digest != digest
        || invocation.grant.tool_name != invocation.prepared.tool_name()
        || invocation.grant.tool_revision != invocation.prepared.tool_revision()
    {
        Err(BatchRuntimeError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn fact(
    context: &EffectBatchAdmissionContext,
    id: &str,
    kind: &str,
    schema_version: u32,
    tool: Option<&str>,
    payload: Value,
) -> Result<FactDraft, BatchRuntimeError> {
    Ok(FactDraft {
        fact_id: FactId::try_from(id).map_err(|_| BatchRuntimeError::InvalidBinding)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: Some(context.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: tool
            .map(LedgerToolId::try_from)
            .transpose()
            .map_err(|_| BatchRuntimeError::InvalidBinding)?,
        kind: FactKind::new(kind).map_err(|_| BatchRuntimeError::InvalidBinding)?,
        schema_version,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| BatchRuntimeError::InvalidBinding)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn content(value: &Value) -> Result<Value, BatchRuntimeError> {
    let canonical =
        CanonicalPayload::from_value(value).map_err(|_| BatchRuntimeError::InvalidBinding)?;
    Ok(json!({"digest":canonical.sha256(),"inline_utf8":canonical.as_json()}))
}

fn child_id(plan: &str, invocation: &str, kind: &str) -> String {
    format!(
        "fact-{}",
        sha256(format!("{plan}:{invocation}:{kind}").as_bytes())
    )
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

const fn replay_class(value: ReplayClass) -> &'static str {
    match value {
        ReplayClass::ReadOnly => "read_only",
        ReplayClass::Idempotent => "idempotent",
        ReplayClass::ReceiptRecoverable => "receipt_recoverable",
        ReplayClass::NeverReplay => "never_replay",
    }
}
