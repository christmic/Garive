//! Runtime-owned durable planning for one bounded parent/child delegation.

use std::{error::Error, fmt};

use garive_core::UsageSummary;
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, ExecutionId,
    FactDraft, FactId, FactKind, TurnId,
};
use garive_llm::TokenCount;
use garive_multiagent::{
    authorize_delegation, ChildRequirement, DelegationAllowance, DelegationAuthorization,
    DelegationErrorCode, DelegationIntent, DelegationResult,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{EffectiveRuntimeLimits, RuntimeCommandError};

/// Runtime failure preserving portable MA0 classifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DelegationRuntimeError {
    /// Portable Engine contract, authority, or budget failure.
    Contract(DelegationErrorCode),
    /// Runtime command construction or durable payload failure.
    Runtime(RuntimeCommandError),
}
impl fmt::Display for DelegationRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(code) => formatter.write_str(code.wire_name()),
            Self::Runtime(error) => write!(formatter, "{error}"),
        }
    }
}
impl Error for DelegationRuntimeError {}

/// Exact child identities and frozen inputs used for the atomic start boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationChildStartCommand {
    /// Resolved or allocated child Agent instance.
    pub child_agent_instance_id: AgentInstanceId,
    /// Single v1 child Turn identity.
    pub child_turn_id: TurnId,
    /// First disposable child Execution identity.
    pub child_execution_id: ExecutionId,
    /// Exact installed child definition.
    pub child_definition_id: AgentDefinitionId,
    /// Exact installed child definition revision.
    pub child_definition_revision: AgentDefinitionRevision,
    /// Effective child snapshot digest.
    pub child_snapshot_digest: String,
    /// Redacted objective bytes resolved by Runtime.
    pub resolved_objective: String,
    /// Parent iteration cursor closed by the suspension.
    pub parent_completed_iterations: u64,
    /// Parent cumulative usage at suspension.
    pub parent_usage: UsageSummary,
    /// Limits frozen into the first child Execution.
    pub child_limits: EffectiveRuntimeLimits,
    /// Fixed Session prefix visible to the child.
    pub through_position: u64,
    /// Explicit Runtime clock observation.
    pub recorded_at: String,
}

/// Plans the durable request that must precede authorization.
pub fn plan_delegation_request(
    intent: &DelegationIntent,
    recorded_at: &str,
) -> Result<FactDraft, DelegationRuntimeError> {
    validate_time(recorded_at)?;
    let binding = intent.intent_binding().map_err(contract)?;
    parent_fact(
        intent,
        "delegation.requested",
        json!({
            "delegation_id":intent.delegation_id(), "parent_agent_instance_id":intent.parent_agent_instance_id(),
            "intent":{"digest":binding.digest,"inline_utf8":binding.inline_utf8},
            "intent_digest":intent.intent_digest().map_err(contract)?, "through_position":intent.through_position(),
        }),
        recorded_at,
    )
}

/// Checks portable bounds and plans the exact durable authority grant.
#[allow(clippy::too_many_arguments)]
pub fn plan_delegation_authorization(
    intent: &DelegationIntent,
    grant_id: &str,
    authority_revision: &str,
    current_depth: u64,
    active_parent_delegations: u64,
    allowance: &DelegationAllowance,
    recorded_at: &str,
) -> Result<(DelegationAuthorization, FactDraft), DelegationRuntimeError> {
    validate_time(recorded_at)?;
    let authorization = authorize_delegation(
        intent,
        grant_id,
        authority_revision,
        current_depth,
        active_parent_delegations,
        allowance,
    )
    .map_err(contract)?;
    let budget = authorization.grant.reserved_budget();
    let fact = parent_fact(
        intent,
        "delegation.authorized",
        json!({
            "delegation_id":intent.delegation_id(), "grant_id":authorization.grant.grant_id(),
            "intent_digest":authorization.grant.intent_digest(), "reserved_budget":budget_json(budget),
            "authority_revision":authorization.grant.authority_revision(),
        }),
        recorded_at,
    )?;
    Ok((authorization, fact))
}

/// Plans an immediate durable denial without allocating or starting a child.
pub fn plan_delegation_denial(
    intent: &DelegationIntent,
    code: DelegationErrorCode,
    recorded_at: &str,
) -> Result<FactDraft, DelegationRuntimeError> {
    validate_time(recorded_at)?;
    parent_fact(
        intent,
        "delegation.denied",
        json!({
            "delegation_id":intent.delegation_id(), "intent_digest":intent.intent_digest().map_err(contract)?, "code":code.wire_name(),
        }),
        recorded_at,
    )
}

/// Plans the atomic parent suspension, child Turn/Execution start, and delegation binding.
pub fn plan_delegation_child_start(
    intent: &DelegationIntent,
    authorization: &DelegationAuthorization,
    command: &DelegationChildStartCommand,
) -> Result<Vec<FactDraft>, DelegationRuntimeError> {
    validate_time(&command.recorded_at)?;
    command.child_limits.validate().map_err(runtime)?;
    validate_digest(&command.child_snapshot_digest)?;
    if authorization.grant.intent_digest() != intent.intent_digest().map_err(contract)?
        || digest(command.resolved_objective.as_bytes()) != intent.objective().digest()
        || command.resolved_objective.len() as u64 > intent.budget().max_objective_bytes
        || !child_matches(intent.child_requirement(), command)
    {
        return Err(DelegationRuntimeError::Contract(
            DelegationErrorCode::DelegationConflict,
        ));
    }
    let parent_turn: TurnId = identity(intent.parent_turn_id())?;
    let parent_execution: ExecutionId = identity(intent.parent_execution_id())?;
    let suspension = suspension_id(intent.delegation_id(), authorization.grant.grant_id());
    let continuation = canonical_content(
        &json!({"delegation_id":intent.delegation_id(),"grant_id":authorization.grant.grant_id()}),
    )?;
    let objective_digest = digest(command.resolved_objective.as_bytes());
    let seed = format!(
        "{}:{}",
        intent.delegation_id(),
        authorization.grant.grant_id()
    );
    Ok(vec![
        fact(
            &seed,
            "execution.suspended",
            Some(&parent_turn),
            Some(&parent_execution),
            json!({"suspension_id":suspension,"reason":"delegation_pending","continuation":continuation,"usage":usage(command.parent_usage),"completed_iterations":command.parent_completed_iterations}),
            &command.recorded_at,
        )?,
        fact(
            &seed,
            "turn.suspended",
            Some(&parent_turn),
            None,
            json!({"suspension_id":suspension,"execution_id":parent_execution.as_str(),"reason":"delegation_pending","continuation":continuation,"cumulative_usage":usage(command.parent_usage)}),
            &command.recorded_at,
        )?,
        fact(
            &seed,
            "turn.started",
            Some(&command.child_turn_id),
            None,
            json!({"command_id":format!("delegation-{}",intent.delegation_id()),"kind":"start","agent_instance_id":command.child_agent_instance_id.as_str(),"definition_id":command.child_definition_id.as_str(),"definition_revision":command.child_definition_revision.as_str(),"snapshot_digest":command.child_snapshot_digest,"trusted_input_digest":objective_digest}),
            &command.recorded_at,
        )?,
        fact(
            &seed,
            "turn.input",
            Some(&command.child_turn_id),
            None,
            json!({"input_kind":"trusted_system","content":{"digest":objective_digest,"inline_utf8":command.resolved_objective}}),
            &command.recorded_at,
        )?,
        fact(
            &seed,
            "execution.started",
            Some(&command.child_turn_id),
            Some(&command.child_execution_id),
            json!({"snapshot_digest":command.child_snapshot_digest,"through_position":command.through_position,"completed_iterations":0,"limits":limits(command.child_limits),"recovery_ordinal":0}),
            &command.recorded_at,
        )?,
        parent_fact(
            intent,
            "delegation.child_started",
            json!({"delegation_id":intent.delegation_id(),"grant_id":authorization.grant.grant_id(),"suspension_id":suspension,"child_agent_instance_id":command.child_agent_instance_id.as_str(),"child_turn_id":command.child_turn_id.as_str(),"child_snapshot_digest":command.child_snapshot_digest}),
            &command.recorded_at,
        )?,
    ])
}

/// Appends the governed result binding to an atomic child terminal transaction.
pub fn plan_delegation_child_terminal(
    intent: &DelegationIntent,
    result: &DelegationResult,
    mut child_terminal_facts: Vec<FactDraft>,
    recorded_at: &str,
) -> Result<Vec<FactDraft>, DelegationRuntimeError> {
    validate_time(recorded_at)?;
    let context = result.context();
    let child_turn: TurnId = identity(&context.child_turn_id)?;
    if context.delegation_id != intent.delegation_id()
        || child_terminal_facts
            .iter()
            .filter(|fact| {
                fact.turn_id.as_ref() == Some(&child_turn)
                    && matches!(
                        fact.kind.as_str(),
                        "turn.completed" | "turn.stopped" | "turn.failed"
                    )
            })
            .count()
            != 1
    {
        return Err(DelegationRuntimeError::Contract(
            DelegationErrorCode::ChildStateCorrupt,
        ));
    }
    let binding = result.result_binding().map_err(contract)?;
    child_terminal_facts.push(parent_fact(intent, "delegation.child_terminal", json!({
        "delegation_id":context.delegation_id,"grant_id":context.grant_id,"result_id":context.result_id,
        "suspension_id":suspension_id(&context.delegation_id,&context.grant_id),"child_agent_instance_id":context.child_agent_instance_id,
        "child_turn_id":context.child_turn_id,"result":{"digest":binding.digest(),"inline_utf8":binding.inline_utf8()},"result_digest":binding.digest(),
    }), recorded_at)?);
    Ok(child_terminal_facts)
}

/// Plans the bounded parent observation after the child terminal commits.
pub fn plan_delegation_observation(
    intent: &DelegationIntent,
    result: &DelegationResult,
    recorded_at: &str,
) -> Result<FactDraft, DelegationRuntimeError> {
    validate_time(recorded_at)?;
    let context = result.context();
    if context.delegation_id != intent.delegation_id() {
        return Err(DelegationRuntimeError::Contract(
            DelegationErrorCode::DelegationConflict,
        ));
    }
    let result_digest = result
        .result_binding()
        .map_err(contract)?
        .digest()
        .to_owned();
    parent_fact(
        intent,
        "delegation.observed",
        json!({"delegation_id":context.delegation_id,"grant_id":context.grant_id,"result_id":context.result_id,"suspension_id":suspension_id(&context.delegation_id,&context.grant_id),"result_digest":result_digest}),
        recorded_at,
    )
}

fn child_matches(requirement: &ChildRequirement, command: &DelegationChildStartCommand) -> bool {
    match requirement {
        ChildRequirement::Existing {
            child_agent_instance_id,
        } => child_agent_instance_id == command.child_agent_instance_id.as_str(),
        ChildRequirement::Definition {
            definition_id,
            definition_revision,
        } => {
            definition_id == command.child_definition_id.as_str()
                && definition_revision == command.child_definition_revision.as_str()
        }
    }
}
fn parent_fact(
    intent: &DelegationIntent,
    kind: &str,
    payload: Value,
    time: &str,
) -> Result<FactDraft, DelegationRuntimeError> {
    let turn = identity(intent.parent_turn_id())?;
    let execution = identity(intent.parent_execution_id())?;
    fact(
        intent.delegation_id(),
        kind,
        Some(&turn),
        Some(&execution),
        payload,
        time,
    )
}
fn fact(
    seed: &str,
    kind: &str,
    turn: Option<&TurnId>,
    execution: Option<&ExecutionId>,
    payload: Value,
    time: &str,
) -> Result<FactDraft, DelegationRuntimeError> {
    Ok(FactDraft {
        fact_id: FactId::try_from(
            format!("fact-{}", digest(format!("{seed}:{kind}").as_bytes())).as_str(),
        )
        .map_err(|_| runtime(RuntimeCommandError::InvalidCommand))?,
        turn_id: turn.cloned(),
        execution_id: execution.cloned(),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| runtime(RuntimeCommandError::InvalidCommand))?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| runtime(RuntimeCommandError::InvalidCommand))?,
        recorded_at: time.to_owned(),
    })
}
fn budget_json(value: &garive_multiagent::DelegationBudget) -> Value {
    json!({"max_child_turns":value.max_child_turns,"max_child_executions":value.max_child_executions,"max_iterations":value.max_iterations,"max_input_tokens":value.max_input_tokens,"max_output_tokens":value.max_output_tokens,"deadline_budget_ms":value.deadline_budget_ms,"max_depth":value.max_depth,"max_objective_bytes":value.max_objective_bytes,"max_input_evidence":value.max_input_evidence,"max_result_schema_bytes":value.max_result_schema_bytes,"max_result_bytes":value.max_result_bytes,"max_result_evidence":value.max_result_evidence})
}
fn limits(value: EffectiveRuntimeLimits) -> Value {
    let mut map =
        serde_json::Map::from_iter([("max_iterations".into(), json!(value.max_iterations))]);
    for (key, item) in [
        ("max_input_tokens", value.max_input_tokens),
        ("max_output_tokens", value.max_output_tokens),
        ("deadline_budget_ms", value.deadline_budget_ms),
    ] {
        if let Some(item) = item {
            map.insert(key.into(), json!(item));
        }
    }
    Value::Object(map)
}
fn usage(value: UsageSummary) -> Value {
    json!({"input_tokens":token(value.input_tokens),"output_tokens":token(value.output_tokens),"source":if value.estimated{"estimated"}else{"provider_reported"}})
}
fn token(value: TokenCount) -> Value {
    match value {
        TokenCount::Known(value) => json!({"kind":"known","value":value}),
        TokenCount::Unknown => json!({"kind":"unknown"}),
    }
}
fn canonical_content(value: &Value) -> Result<Value, DelegationRuntimeError> {
    let bytes =
        serde_jcs::to_vec(value).map_err(|_| runtime(RuntimeCommandError::InvalidCommand))?;
    let text =
        String::from_utf8(bytes).map_err(|_| runtime(RuntimeCommandError::InvalidCommand))?;
    Ok(json!({"digest":digest(text.as_bytes()),"inline_utf8":text}))
}
fn suspension_id(delegation_id: &str, grant_id: &str) -> String {
    format!(
        "delegation-suspension-{}",
        digest(format!("{delegation_id}:{grant_id}").as_bytes())
    )
}
fn validate_digest(value: &str) -> Result<(), DelegationRuntimeError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(runtime(RuntimeCommandError::InvalidCommand))
    }
}
fn validate_time(value: &str) -> Result<(), DelegationRuntimeError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| runtime(RuntimeCommandError::InvalidCommand))
}
fn identity<'a, T: TryFrom<&'a str>>(value: &'a str) -> Result<T, DelegationRuntimeError> {
    T::try_from(value).map_err(|_| runtime(RuntimeCommandError::InvalidCommand))
}
fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn contract(error: garive_multiagent::DelegationError) -> DelegationRuntimeError {
    DelegationRuntimeError::Contract(error.code())
}
const fn runtime(error: RuntimeCommandError) -> DelegationRuntimeError {
    DelegationRuntimeError::Runtime(error)
}
