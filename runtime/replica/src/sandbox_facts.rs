//! Durable F0 fact planning before an external-effect Started boundary.

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, ToolInvocationId as LedgerToolId,
    TurnId,
};
use garive_tools::{InvocationGrant, PreparedToolCall, ReplayClass};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    preflight_sandbox, PreparedExecution, SafetyDecisionV1, SafetyDisposition, SafetyRequestV1,
    SandboxBindingV1, SandboxPreflightError,
};

/// Frozen ownership, authority context and audit identities for one F0 admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct F0EffectAdmissionContext {
    /// Turn owning the invocation.
    pub turn_id: TurnId,
    /// Active Execution owning the invocation.
    pub execution_id: ExecutionId,
    /// Stable Sandbox preflight identity.
    pub preflight_id: String,
    /// Digest of effective post-narrowing limits.
    pub effective_limits_digest: String,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Exact pre-start fact batch and executor selection admitted by F0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedF0EffectAdmission {
    /// Executor/attempt binding that the later Started fact must repeat.
    pub execution: PreparedExecution,
    /// Ordered Prepared-v3, Safety, Grant-v2, Binding and Preflight facts.
    pub facts: Vec<FactDraft>,
}

/// Plans the complete allowed F0 chain without crossing the dispatch boundary.
#[allow(clippy::too_many_arguments)]
pub fn plan_f0_effect_admission(
    context: &F0EffectAdmissionContext,
    request: &SafetyRequestV1,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
    decision: &SafetyDecisionV1,
    binding: &SandboxBindingV1,
    dispatch_attempt_id: &str,
) -> Result<PlannedF0EffectAdmission, SandboxPreflightError> {
    validate_context(context)?;
    let invocation_id = request.invocation_id();
    if decision.disposition() != SafetyDisposition::Allow
        || decision.invocation_id() != invocation_id
        || decision.prepared_digest() != prepared.input_digest()
        || request.prepared_digest() != prepared.input_digest()
        || request.tool_name() != prepared.tool_name()
        || request.tool_revision() != prepared.tool_revision()
        || request.sandbox_requirements_digest()
            != prepared
                .sandbox_requirements_digest()
                .ok_or(SandboxPreflightError::InvalidBinding)?
        || request.effective_policy_revision() != decision.policy_revision()
    {
        return Err(SandboxPreflightError::InvalidBinding);
    }
    let execution = preflight_sandbox(
        invocation_id,
        prepared,
        grant,
        decision,
        binding,
        dispatch_attempt_id,
    )?;
    let accesses = prepared
        .invocation_accesses()
        .ok_or(SandboxPreflightError::InvalidBinding)?;
    let requirements = prepared
        .sandbox_requirements()
        .ok_or(SandboxPreflightError::InvalidBinding)?;
    let access = content(accesses)?;
    if access["digest"] != request.exact_access_digest() {
        return Err(SandboxPreflightError::InvalidBinding);
    }
    let sandbox = content(requirements)?;
    let granted = content(&grant.granted_requirements)?;
    let access_scope_digest = canonical_digest(binding.access_scope())?;
    let enforcement_digest = binding
        .enforcement()
        .digest()
        .map_err(|_| SandboxPreflightError::InvalidBinding)?;
    let tool = LedgerToolId::try_from(invocation_id.as_str())
        .map_err(|_| SandboxPreflightError::InvalidBinding)?;
    let common = |suffix: &str, kind: &str, schema_version: u32, payload: Value| {
        fact(context, &tool, suffix, kind, schema_version, payload)
    };
    let mut safety = json!({
        "request_id":request.request_id(),"decision_id":decision.decision_id(),
        "disposition":"allow","prepared_digest":prepared.input_digest(),
        "tool_name":prepared.tool_name(),"tool_revision":prepared.tool_revision(),
        "actor_authority_reference":request.actor_authority_reference(),
        "exact_access_digest":access["digest"],
        "sandbox_requirements_digest":sandbox["digest"],
        "policy_revision":decision.policy_revision(),
        "constraints_digest":decision.constraints_digest().ok_or(SandboxPreflightError::InvalidBinding)?,
    });
    if let Some(value) = request.goal_reference() {
        safety["goal_reference"] = json!(value);
    }
    if let Some(value) = request.plan_reference() {
        safety["plan_reference"] = json!(value);
    }
    let facts = vec![
        common(
            "prepared",
            "effect.prepared",
            3,
            json!({
                "prepared_contract_version":3,"prepared_digest":prepared.input_digest(),
                "tool_name":prepared.tool_name(),"tool_revision":prepared.tool_revision(),
                "replay_class":replay_class(prepared.replay_class()),"model_call_id":prepared.model_call_id(),
                "access_policy_revision":prepared.access_policy_revision().ok_or(SandboxPreflightError::InvalidBinding)?,
                "access_resolver_revision":prepared.access_resolver_revision().ok_or(SandboxPreflightError::InvalidBinding)?,
                "invocation_accesses":access,"max_result_bytes":prepared.max_result_bytes().ok_or(SandboxPreflightError::InvalidBinding)?,
                "sandbox_requirements":sandbox,"sandbox_requirements_digest":prepared.sandbox_requirements_digest().ok_or(SandboxPreflightError::InvalidBinding)?,
            }),
        )?,
        common("safety", "safety.decided", 1, safety)?,
        common(
            "authorized",
            "effect.authorized",
            2,
            json!({
                "prepared_contract_version":3,"prepared_digest":prepared.input_digest(),
                "grant_id":grant.grant_id.as_str(),"authority_revision":grant.authority_revision,
                "constraints_digest":grant.constraints_digest,"granted_requirements":granted,
            }),
        )?,
        common(
            "bound",
            "sandbox.bound",
            1,
            json!({
                "binding_id":binding.binding_id(),"decision_id":decision.decision_id(),
                "prepared_digest":prepared.input_digest(),"workspace_capability_id":binding.workspace_capability_id(),
                "executor_id":binding.executor_id(),"executor_revision":binding.executor_revision(),
                "policy_revision":binding.policy_revision(),"access_scope_digest":access_scope_digest,
                "enforcement_digest":enforcement_digest,"effective_limits_digest":context.effective_limits_digest,
            }),
        )?,
        common(
            "preflight",
            "sandbox.preflighted",
            1,
            json!({
                "preflight_id":context.preflight_id,"binding_id":binding.binding_id(),
                "decision_id":decision.decision_id(),"prepared_digest":prepared.input_digest(),
                "grant_id":grant.grant_id.as_str(),"executor_id":execution.executor_id,
                "executor_revision":execution.executor_revision,"dispatch_attempt_id":execution.dispatch_attempt_id,
            }),
        )?,
    ];
    Ok(PlannedF0EffectAdmission { execution, facts })
}

fn validate_context(value: &F0EffectAdmissionContext) -> Result<(), SandboxPreflightError> {
    if value.preflight_id.is_empty()
        || value.effective_limits_digest.len() != 64
        || chrono::DateTime::parse_from_rfc3339(&value.recorded_at).is_err()
    {
        Err(SandboxPreflightError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn fact(
    context: &F0EffectAdmissionContext,
    tool: &LedgerToolId,
    suffix: &str,
    kind: &str,
    schema_version: u32,
    payload: Value,
) -> Result<FactDraft, SandboxPreflightError> {
    Ok(FactDraft {
        fact_id: FactId::try_from(
            format!(
                "fact-{}",
                sha256(format!("{}:{suffix}", tool.as_str()).as_bytes())
            )
            .as_str(),
        )
        .map_err(|_| SandboxPreflightError::InvalidBinding)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: Some(context.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: Some(tool.clone()),
        kind: FactKind::new(kind).map_err(|_| SandboxPreflightError::InvalidBinding)?,
        schema_version,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| SandboxPreflightError::InvalidBinding)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn content(value: &impl Serialize) -> Result<Value, SandboxPreflightError> {
    let value = serde_json::to_value(value).map_err(|_| SandboxPreflightError::InvalidBinding)?;
    let payload =
        CanonicalPayload::from_value(&value).map_err(|_| SandboxPreflightError::InvalidBinding)?;
    Ok(json!({"digest":payload.sha256(),"inline_utf8":payload.as_json()}))
}

fn canonical_digest(value: &impl Serialize) -> Result<String, SandboxPreflightError> {
    serde_jcs::to_vec(value)
        .map(|bytes| sha256(&bytes))
        .map_err(|_| SandboxPreflightError::InvalidBinding)
}

fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

const fn replay_class(value: ReplayClass) -> &'static str {
    match value {
        ReplayClass::ReadOnly => "read_only",
        ReplayClass::Idempotent => "idempotent",
        ReplayClass::ReceiptRecoverable => "receipt_recoverable",
        ReplayClass::NeverReplay => "never_replay",
    }
}
