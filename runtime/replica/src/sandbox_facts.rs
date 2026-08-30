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

/// Durable ownership and time known when Safety evaluates Prepared-v3.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct F0SafetyDecisionContext {
    /// Turn owning the invocation.
    pub turn_id: TurnId,
    /// Active Execution owning the invocation.
    pub execution_id: ExecutionId,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

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

/// Plans Prepared-v3 and the exact allowed Safety decision before grant creation.
pub fn plan_f0_safety_decision(
    context: &F0SafetyDecisionContext,
    request: &SafetyRequestV1,
    prepared: &PreparedToolCall,
    decision: &SafetyDecisionV1,
) -> Result<Vec<FactDraft>, SandboxPreflightError> {
    validate_request(&context.recorded_at, request, prepared, decision)?;
    let accesses = prepared
        .invocation_accesses()
        .ok_or(SandboxPreflightError::InvalidBinding)?;
    let requirements = prepared
        .sandbox_requirements()
        .ok_or(SandboxPreflightError::InvalidBinding)?;
    let access = content(accesses)?;
    let sandbox = content(requirements)?;
    if access["digest"] != request.exact_access_digest()
        || sandbox["digest"] != request.sandbox_requirements_digest()
    {
        return Err(SandboxPreflightError::InvalidBinding);
    }
    let tool = ledger_tool(request)?;
    let mut safety = json!({
        "request_id":request.request_id(),"decision_id":decision.decision_id(),
        "disposition":safety_disposition(decision.disposition()),"prepared_digest":prepared.input_digest(),
        "tool_name":prepared.tool_name(),"tool_revision":prepared.tool_revision(),
        "actor_authority_reference":request.actor_authority_reference(),
        "exact_access_digest":access["digest"],
        "sandbox_requirements_digest":sandbox["digest"],
        "policy_revision":decision.policy_revision(),
    });
    match decision.disposition() {
        SafetyDisposition::Allow => {
            safety["constraints_digest"] = json!(decision
                .constraints_digest()
                .ok_or(SandboxPreflightError::InvalidBinding)?);
        }
        SafetyDisposition::Deny | SafetyDisposition::InteractionRequired => {
            safety["safe_code"] = json!(decision
                .safe_code()
                .ok_or(SandboxPreflightError::InvalidBinding)?);
        }
    }
    if let Some(value) = request.goal_reference() {
        safety["goal_reference"] = json!(value);
    }
    if let Some(value) = request.plan_reference() {
        safety["plan_reference"] = json!(value);
    }
    Ok(vec![
        fact(
            &context.turn_id,
            &context.execution_id,
            &context.recorded_at,
            &tool,
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
        fact(
            &context.turn_id,
            &context.execution_id,
            &context.recorded_at,
            &tool,
            "safety",
            "safety.decided",
            1,
            safety,
        )?,
    ])
}

/// Plans Grant-v2, concrete binding and preflight after Safety is durable.
#[allow(clippy::too_many_arguments)]
pub fn plan_f0_sandbox_admission(
    context: &F0EffectAdmissionContext,
    request: &SafetyRequestV1,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
    decision: &SafetyDecisionV1,
    binding: &SandboxBindingV1,
    dispatch_attempt_id: &str,
) -> Result<PlannedF0EffectAdmission, SandboxPreflightError> {
    validate_context(context)?;
    validate_request(&context.recorded_at, request, prepared, decision)?;
    if decision.disposition() != SafetyDisposition::Allow {
        return Err(SandboxPreflightError::DecisionNotAllowed);
    }
    let execution = preflight_sandbox(
        request.invocation_id(),
        prepared,
        grant,
        decision,
        binding,
        dispatch_attempt_id,
    )?;
    let granted = content(&grant.granted_requirements)?;
    let access_scope_digest = canonical_digest(binding.access_scope())?;
    let enforcement_digest = binding
        .enforcement()
        .digest()
        .map_err(|_| SandboxPreflightError::InvalidBinding)?;
    let tool = ledger_tool(request)?;
    let facts = vec![
        fact(
            &context.turn_id,
            &context.execution_id,
            &context.recorded_at,
            &tool,
            "authorized",
            "effect.authorized",
            2,
            json!({
                "prepared_contract_version":3,"prepared_digest":prepared.input_digest(),
                "grant_id":grant.grant_id.as_str(),"authority_revision":grant.authority_revision,
                "constraints_digest":grant.constraints_digest,"granted_requirements":granted,
            }),
        )?,
        fact(
            &context.turn_id,
            &context.execution_id,
            &context.recorded_at,
            &tool,
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
        fact(
            &context.turn_id,
            &context.execution_id,
            &context.recorded_at,
            &tool,
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
    let mut safety = plan_f0_safety_decision(
        &F0SafetyDecisionContext {
            turn_id: context.turn_id.clone(),
            execution_id: context.execution_id.clone(),
            recorded_at: context.recorded_at.clone(),
        },
        request,
        prepared,
        decision,
    )?;
    let mut planned = plan_f0_sandbox_admission(
        context,
        request,
        prepared,
        grant,
        decision,
        binding,
        dispatch_attempt_id,
    )?;
    safety.append(&mut planned.facts);
    planned.facts = safety;
    Ok(planned)
}

fn validate_request(
    recorded_at: &str,
    request: &SafetyRequestV1,
    prepared: &PreparedToolCall,
    decision: &SafetyDecisionV1,
) -> Result<(), SandboxPreflightError> {
    if chrono::DateTime::parse_from_rfc3339(recorded_at).is_err()
        || decision.invocation_id() != request.invocation_id()
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
        Err(SandboxPreflightError::InvalidBinding)
    } else {
        Ok(())
    }
}

const fn safety_disposition(value: SafetyDisposition) -> &'static str {
    match value {
        SafetyDisposition::Allow => "allow",
        SafetyDisposition::Deny => "deny",
        SafetyDisposition::InteractionRequired => "interaction_required",
    }
}

fn ledger_tool(request: &SafetyRequestV1) -> Result<LedgerToolId, SandboxPreflightError> {
    LedgerToolId::try_from(request.invocation_id().as_str())
        .map_err(|_| SandboxPreflightError::InvalidBinding)
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

#[allow(clippy::too_many_arguments)]
fn fact(
    turn_id: &TurnId,
    execution_id: &ExecutionId,
    recorded_at: &str,
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
        turn_id: Some(turn_id.clone()),
        execution_id: Some(execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: Some(tool.clone()),
        kind: FactKind::new(kind).map_err(|_| SandboxPreflightError::InvalidBinding)?,
        schema_version,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| SandboxPreflightError::InvalidBinding)?,
        recorded_at: recorded_at.into(),
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
