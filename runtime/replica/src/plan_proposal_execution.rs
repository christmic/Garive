//! Durable binding of one completed internal Planner result.

use garive_ledger::{
    CanonicalPayload, DurableFact, FactDraft, FactId, FactKind, SessionId, TurnId,
};
use garive_plan::{PlanBoundsV1, PlanCapabilityReference, PlanStepId, PlanStepV1};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{PlanProposalContent, SqliteLedger};

const PROPOSAL_CONTRACT: &str = "garive.plan-proposal-topology";
const PROPOSAL_VERSION: u8 = 1;

/// Result bytes recovered from a committed Planner terminal and its binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlanProposalResult {
    /// Durable result-binding fact identity.
    pub binding_fact_id: String,
    /// Position of the existing or newly committed binding.
    pub binding_position: u64,
    /// Goal identity frozen by the original Planner request.
    pub goal_id: String,
    /// Goal lifecycle revision frozen by the original request.
    pub goal_revision: u64,
    /// Immutable Goal definition digest frozen by the original request.
    pub goal_definition_digest: String,
    /// Stable secret-free Planner revision frozen by the original request.
    pub proposer_reference: String,
    /// Digest shared by both C6 completed terminals.
    pub result_digest: String,
    /// Canonical model-item array read from the durable terminal fact.
    pub response_items_json: String,
    /// Exact source authority when this result proposes revision N+1.
    pub replan: Option<BoundPlanReplanResult>,
}

/// Durable source binding copied from one admitted replan request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlanReplanResult {
    /// Failure-policy admission fact authorizing the proposal.
    pub admission_fact_id: String,
    /// Source Plan identity retained by the next revision.
    pub source_plan_id: String,
    /// Exact immutable source revision.
    pub source_plan_revision: u64,
    /// Canonical source Plan definition digest.
    pub source_plan_definition_digest: String,
}

/// Stable failures while binding a Planner terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanProposalBindingError {
    /// Caller identity, version or time is malformed.
    InvalidInput,
    /// The fixed prefix is absent, ambiguous, stale or corrupt.
    CorruptState,
    /// Another writer changed the Session before binding committed.
    ConcurrentModification,
    /// SQLite could not read or commit the durable state.
    DurabilityFailure,
}

/// Identifies one exact internal Planner Execution from the current Ledger.
pub fn is_plan_proposal_execution(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    turn_id: &TurnId,
    execution_id: &garive_ledger::ExecutionId,
) -> Result<bool, PlanProposalBindingError> {
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(|_| PlanProposalBindingError::DurabilityFailure)?
        .ok_or(PlanProposalBindingError::CorruptState)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(|_| PlanProposalBindingError::DurabilityFailure)?;
    let matching = facts
        .iter()
        .filter(|fact| {
            matches!(
                fact.kind.as_str(),
                "plan.proposal.requested" | "plan.replan.proposal.requested"
            )
        })
        .filter(|fact| {
            payload(fact).ok().is_some_and(|value| {
                value.get("turn_id").and_then(Value::as_str) == Some(turn_id.as_str())
                    && value.get("execution_id").and_then(Value::as_str)
                        == Some(execution_id.as_str())
            })
        })
        .count();
    match matching {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(PlanProposalBindingError::CorruptState),
    }
}

/// Stable failure while reducing one bound model result to topology only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanProposalResultError {
    /// The terminal response is not one exact canonical topology document.
    InvalidOutput,
}

/// Canonical JSON Schema and digest frozen into every Planner request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanProposalOutputSchema {
    /// RFC 8785 schema document supplied to the model contract.
    pub canonical_json: String,
    /// Lowercase SHA-256 over `canonical_json`.
    pub digest: String,
}

/// Constructs the one closed topology schema owned by Runtime.
pub fn plan_proposal_output_schema() -> PlanProposalOutputSchema {
    let capability = json!({
        "type":"object", "additionalProperties":false,
        "required":["name","exact_revision"],
        "properties":{"name":{"type":"string","minLength":1},
            "exact_revision":{"type":"string","minLength":1}}
    });
    let schema = json!({
        "type":"object", "additionalProperties":false,
        "required":["contract","version","steps","bounds"],
        "properties":{
            "contract":{"const":PROPOSAL_CONTRACT}, "version":{"const":PROPOSAL_VERSION},
            "steps":{"type":"array","minItems":1,"items":{
                "type":"object","additionalProperties":false,
                "required":["step_id","objective","depends_on","completion_criteria",
                    "required_capabilities","input_bindings","max_attempts"],
                "properties":{
                    "step_id":{"type":"string","minLength":1},
                    "objective":{"type":"string","minLength":1},
                    "depends_on":{"type":"array","items":{"type":"string","minLength":1}},
                    "completion_criteria":{"type":"array","minItems":1,
                        "items":{"type":"string","minLength":1}},
                    "required_capabilities":{"type":"array","items":capability},
                    "input_bindings":{"type":"array","items":{"type":"string","pattern":"^[0-9a-f]{64}$"}},
                    "max_attempts":{"type":"integer","minimum":1}
                }
            }},
            "bounds":{"type":"object","additionalProperties":false,
                "required":["max_steps","max_parallel_ready","max_total_attempts",
                    "token_budget","duration_budget_ms"],
                "properties":{
                    "max_steps":{"type":"integer","minimum":1},
                    "max_parallel_ready":{"type":"integer","minimum":1},
                    "max_total_attempts":{"type":"integer","minimum":1},
                    "token_budget":{"type":["integer","null"],"minimum":1},
                    "duration_budget_ms":{"type":["integer","null"],"minimum":1}
                }
            }
        }
    });
    let canonical = CanonicalPayload::from_value(&schema).expect("static schema is canonical");
    PlanProposalOutputSchema {
        canonical_json: canonical.as_json().into(),
        digest: canonical.sha256().into(),
    }
}

/// Parses topology only from response bytes recovered by the durable binding.
pub fn parse_bound_plan_proposal_result(
    bound: &BoundPlanProposalResult,
) -> Result<PlanProposalContent, PlanProposalResultError> {
    if digest(bound.response_items_json.as_bytes()) != bound.result_digest {
        return Err(PlanProposalResultError::InvalidOutput);
    }
    let items: Vec<RawModelItem> = serde_json::from_str(&bound.response_items_json)
        .map_err(|_| PlanProposalResultError::InvalidOutput)?;
    let [item] = items.as_slice() else {
        return Err(PlanProposalResultError::InvalidOutput);
    };
    if item.kind != "text" {
        return Err(PlanProposalResultError::InvalidOutput);
    }
    let value: Value =
        serde_json::from_str(&item.text).map_err(|_| PlanProposalResultError::InvalidOutput)?;
    if serde_jcs::to_string(&value).map_err(|_| PlanProposalResultError::InvalidOutput)?
        != item.text
    {
        return Err(PlanProposalResultError::InvalidOutput);
    }
    let raw: RawTopology =
        serde_json::from_value(value).map_err(|_| PlanProposalResultError::InvalidOutput)?;
    if raw.contract != PROPOSAL_CONTRACT || raw.version != PROPOSAL_VERSION {
        return Err(PlanProposalResultError::InvalidOutput);
    }
    let bounds = PlanBoundsV1::new(
        raw.bounds.max_steps,
        raw.bounds.max_parallel_ready,
        raw.bounds.max_total_attempts,
        raw.bounds.token_budget,
        raw.bounds.duration_budget_ms,
    )
    .map_err(|_| PlanProposalResultError::InvalidOutput)?;
    if raw.steps.is_empty() || raw.steps.len() > bounds.max_steps() as usize {
        return Err(PlanProposalResultError::InvalidOutput);
    }
    let steps = raw
        .steps
        .into_iter()
        .map(RawStep::build)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlanProposalContent { steps, bounds })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModelItem {
    kind: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTopology {
    contract: String,
    version: u8,
    steps: Vec<RawStep>,
    bounds: RawBounds,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBounds {
    max_steps: u32,
    max_parallel_ready: u32,
    max_total_attempts: u32,
    token_budget: Option<u64>,
    duration_budget_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    step_id: String,
    objective: String,
    depends_on: Vec<String>,
    completion_criteria: Vec<String>,
    required_capabilities: Vec<RawCapability>,
    input_bindings: Vec<String>,
    max_attempts: u32,
}

impl RawStep {
    fn build(self) -> Result<PlanStepV1, PlanProposalResultError> {
        PlanStepV1::new(
            PlanStepId::new(self.step_id).map_err(|_| PlanProposalResultError::InvalidOutput)?,
            self.objective,
            self.depends_on
                .into_iter()
                .map(PlanStepId::new)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| PlanProposalResultError::InvalidOutput)?,
            self.completion_criteria,
            self.required_capabilities
                .into_iter()
                .map(RawCapability::build)
                .collect::<Result<Vec<_>, _>>()?,
            self.input_bindings,
            self.max_attempts,
        )
        .map_err(|_| PlanProposalResultError::InvalidOutput)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCapability {
    name: String,
    exact_revision: String,
}

impl RawCapability {
    fn build(self) -> Result<PlanCapabilityReference, PlanProposalResultError> {
        PlanCapabilityReference::new(self.name, self.exact_revision)
            .map_err(|_| PlanProposalResultError::InvalidOutput)
    }
}

/// Binds one exact completed Planner result before topology parsing.
pub fn bind_completed_plan_proposal_result(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    planner_turn_id: &TurnId,
    expected_session_version: u64,
    recorded_at: &str,
) -> Result<BoundPlanProposalResult, PlanProposalBindingError> {
    if expected_session_version == 0 || chrono::DateTime::parse_from_rfc3339(recorded_at).is_err() {
        return Err(PlanProposalBindingError::InvalidInput);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(|_| PlanProposalBindingError::DurabilityFailure)?
        .ok_or(PlanProposalBindingError::CorruptState)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(|_| PlanProposalBindingError::DurabilityFailure)?;
    let request = one(&facts, |fact| {
        is_proposal_request_kind(fact.kind.as_str())
            && payload(fact).ok().is_some_and(|value| {
                value.get("turn_id").and_then(Value::as_str) == Some(planner_turn_id.as_str())
            })
    })?;
    let request_value = payload(request)?;
    let replan = bound_replan_request(request, &request_value)?;
    let binding_kind = if replan.is_some() {
        "plan.replan.proposal.result_bound"
    } else {
        "plan.proposal.result_bound"
    };
    let execution_id = text(&request_value, "execution_id")?;
    let started = one(&facts, |fact| {
        fact.kind.as_str() == "turn.started" && fact.turn_id.as_ref() == Some(planner_turn_id)
    })?;
    let input = one(&facts, |fact| {
        fact.kind.as_str() == "turn.input" && fact.turn_id.as_ref() == Some(planner_turn_id)
    })?;
    let execution_started = one(&facts, |fact| {
        fact.kind.as_str() == "execution.started"
            && fact.turn_id.as_ref() == Some(planner_turn_id)
            && fact.execution_id.as_ref().map(|value| value.as_str()) == Some(execution_id)
    })?;
    let started_value = payload(started)?;
    let input_value = payload(input)?;
    let execution_started_value = payload(execution_started)?;
    if request.position.checked_add(1) != Some(started.position)
        || started.position.checked_add(1) != Some(input.position)
        || input.position.checked_add(1) != Some(execution_started.position)
        || text(&request_value, "command_id")? != text(&started_value, "command_id")?
        || text(&request_value, "request_digest")? != content(&input_value, "content", "digest")?
        || text(&input_value, "input_kind")? != "trusted_system"
        || number(&request_value, "through_position")?
            != number(&execution_started_value, "through_position")?
    {
        return Err(PlanProposalBindingError::CorruptState);
    }
    let terminal = one(&facts, |fact| {
        fact.kind.as_str() == "turn.completed" && fact.turn_id.as_ref() == Some(planner_turn_id)
    })?;
    let execution_terminal = one(&facts, |fact| {
        fact.kind.as_str() == "execution.completed"
            && fact.turn_id.as_ref() == Some(planner_turn_id)
            && fact.execution_id.as_ref().map(|value| value.as_str()) == Some(execution_id)
    })?;
    let terminal_value = payload(terminal)?;
    let execution_value = payload(execution_terminal)?;
    let result_digest = content(&terminal_value, "response", "digest")?;
    let response_items_json = content(&terminal_value, "response", "inline_utf8")?;
    let terminal_commit = ledger
        .fact_commit_version(&terminal.fact_id)
        .map_err(|_| PlanProposalBindingError::DurabilityFailure)?
        .ok_or(PlanProposalBindingError::CorruptState)?;
    let execution_commit = ledger
        .fact_commit_version(&execution_terminal.fact_id)
        .map_err(|_| PlanProposalBindingError::DurabilityFailure)?
        .ok_or(PlanProposalBindingError::CorruptState)?;
    if text(&terminal_value, "execution_id")? != execution_id
        || content(&execution_value, "response", "digest")? != result_digest
        || content(&execution_value, "response", "inline_utf8")? != response_items_json
        || terminal_commit != execution_commit
    {
        return Err(PlanProposalBindingError::CorruptState);
    }
    let mut bindings = facts.iter().filter(|fact| {
        fact.kind.as_str() == binding_kind
            && payload(fact).ok().is_some_and(|value| {
                value.get("request_fact_id").and_then(Value::as_str)
                    == Some(request.fact_id.as_str())
            })
    });
    if let Some(bound) = bindings.next() {
        if bindings.next().is_some() {
            return Err(PlanProposalBindingError::CorruptState);
        }
        let value = payload(bound)?;
        if text(&value, "terminal_fact_id")? != terminal.fact_id.as_str()
            || text(&value, "result_digest")? != result_digest
            || text(&value, "planner_execution_id")? != execution_id
            || text(&value, "planner_turn_id")? != planner_turn_id.as_str()
            || text(&value, "terminal_payload_digest")? != terminal.payload.sha256()
            || text(&value, "goal_id")? != text(&request_value, "goal_id")?
            || number(&value, "goal_revision")? != number(&request_value, "goal_revision")?
            || text(&value, "goal_definition_digest")?
                != text(&request_value, "goal_definition_digest")?
            || !same_replan_binding(&value, replan.as_ref())?
        {
            return Err(PlanProposalBindingError::CorruptState);
        }
        return result(bound, &request_value, result_digest, response_items_json);
    }
    if watermark.session_version != expected_session_version {
        return Err(PlanProposalBindingError::ConcurrentModification);
    }
    let command_id = format!(
        "planner-bind-{}",
        &digest(format!("{}:{}", request.fact_id.as_str(), terminal.fact_id.as_str()).as_bytes())
            [..32]
    );
    let mut binding_payload = json!({
        "command_id":command_id,
        "goal_id":text(&request_value,"goal_id")?,
        "goal_revision":number(&request_value,"goal_revision")?,
        "goal_definition_digest":text(&request_value,"goal_definition_digest")?,
        "request_fact_id":request.fact_id.as_str(),
        "planner_turn_id":planner_turn_id.as_str(),
        "planner_execution_id":execution_id,
        "terminal_fact_id":terminal.fact_id.as_str(),
        "terminal_payload_digest":terminal.payload.sha256(),
        "result_digest":result_digest,
    });
    if let Some(replan) = &replan {
        binding_payload
            .as_object_mut()
            .ok_or(PlanProposalBindingError::CorruptState)?
            .extend(
                json!({
                    "admission_fact_id":replan.admission_fact_id,
                    "source_plan_id":replan.source_plan_id,
                    "source_plan_revision":replan.source_plan_revision,
                    "source_plan_definition_digest":replan.source_plan_definition_digest,
                })
                .as_object()
                .cloned()
                .ok_or(PlanProposalBindingError::CorruptState)?,
            );
    }
    let fact = FactDraft {
        fact_id: FactId::try_from(format!("fact-{}", digest(command_id.as_bytes())).as_str())
            .map_err(|_| PlanProposalBindingError::InvalidInput)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(binding_kind).map_err(|_| PlanProposalBindingError::InvalidInput)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&binding_payload)
            .map_err(|_| PlanProposalBindingError::CorruptState)?,
        recorded_at: recorded_at.into(),
    };
    let committed = ledger
        .commit(
            session_id.clone(),
            expected_session_version,
            vec![fact.clone()],
        )
        .map_err(|error| match error {
            crate::SqliteLedgerError::Domain(
                garive_ledger::LedgerError::ConcurrentModification,
            ) => PlanProposalBindingError::ConcurrentModification,
            crate::SqliteLedgerError::Storage(_) => PlanProposalBindingError::DurabilityFailure,
            _ => PlanProposalBindingError::CorruptState,
        })?;
    Ok(BoundPlanProposalResult {
        binding_fact_id: fact.fact_id.as_str().into(),
        binding_position: committed.positions[0],
        goal_id: text(&request_value, "goal_id")?.into(),
        goal_revision: number(&request_value, "goal_revision")?,
        goal_definition_digest: text(&request_value, "goal_definition_digest")?.into(),
        proposer_reference: text(&request_value, "proposer_reference")?.into(),
        result_digest: result_digest.into(),
        response_items_json: response_items_json.into(),
        replan,
    })
}

fn result(
    fact: &DurableFact,
    request: &Value,
    digest: &str,
    json: &str,
) -> Result<BoundPlanProposalResult, PlanProposalBindingError> {
    Ok(BoundPlanProposalResult {
        binding_fact_id: fact.fact_id.as_str().into(),
        binding_position: fact.position,
        goal_id: text(request, "goal_id")?.into(),
        goal_revision: number(request, "goal_revision")?,
        goal_definition_digest: text(request, "goal_definition_digest")?.into(),
        proposer_reference: text(request, "proposer_reference")?.into(),
        result_digest: digest.into(),
        response_items_json: json.into(),
        replan: bound_replan_request(fact, request)?,
    })
}

fn is_proposal_request_kind(kind: &str) -> bool {
    matches!(
        kind,
        "plan.proposal.requested" | "plan.replan.proposal.requested"
    )
}

fn bound_replan_request(
    fact: &DurableFact,
    value: &Value,
) -> Result<Option<BoundPlanReplanResult>, PlanProposalBindingError> {
    if !matches!(
        fact.kind.as_str(),
        "plan.replan.proposal.requested" | "plan.replan.proposal.result_bound"
    ) {
        return Ok(None);
    }
    Ok(Some(BoundPlanReplanResult {
        admission_fact_id: text(value, "admission_fact_id")?.into(),
        source_plan_id: text(value, "source_plan_id")?.into(),
        source_plan_revision: number(value, "source_plan_revision")?,
        source_plan_definition_digest: text(value, "source_plan_definition_digest")?.into(),
    }))
}

fn same_replan_binding(
    value: &Value,
    expected: Option<&BoundPlanReplanResult>,
) -> Result<bool, PlanProposalBindingError> {
    let Some(expected) = expected else {
        return Ok(true);
    };
    Ok(
        text(value, "admission_fact_id")? == expected.admission_fact_id
            && text(value, "source_plan_id")? == expected.source_plan_id
            && number(value, "source_plan_revision")? == expected.source_plan_revision
            && text(value, "source_plan_definition_digest")?
                == expected.source_plan_definition_digest,
    )
}

fn one(
    facts: &[DurableFact],
    predicate: impl Fn(&DurableFact) -> bool,
) -> Result<&DurableFact, PlanProposalBindingError> {
    let mut found = facts.iter().filter(|fact| predicate(fact));
    let value = found.next().ok_or(PlanProposalBindingError::CorruptState)?;
    if found.next().is_some() {
        Err(PlanProposalBindingError::CorruptState)
    } else {
        Ok(value)
    }
}

fn payload(fact: &DurableFact) -> Result<Value, PlanProposalBindingError> {
    serde_json::from_str(fact.payload.as_json()).map_err(|_| PlanProposalBindingError::CorruptState)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, PlanProposalBindingError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(PlanProposalBindingError::CorruptState)
}
fn number(value: &Value, key: &str) -> Result<u64, PlanProposalBindingError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(PlanProposalBindingError::CorruptState)
}
fn content<'a>(
    value: &'a Value,
    key: &str,
    field: &str,
) -> Result<&'a str, PlanProposalBindingError> {
    value
        .get(key)
        .and_then(|value| value.get(field))
        .and_then(Value::as_str)
        .ok_or(PlanProposalBindingError::CorruptState)
}
fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
