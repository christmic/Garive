//! Durable binding of one completed internal Planner result.

use garive_ledger::{
    CanonicalPayload, DurableFact, FactDraft, FactId, FactKind, SessionId, TurnId,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::SqliteLedger;

/// Result bytes recovered from a committed Planner terminal and its binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlanProposalResult {
    /// Durable result-binding fact identity.
    pub binding_fact_id: String,
    /// Position of the existing or newly committed binding.
    pub binding_position: u64,
    /// Digest shared by both C6 completed terminals.
    pub result_digest: String,
    /// Canonical model-item array read from the durable terminal fact.
    pub response_items_json: String,
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
        fact.kind.as_str() == "plan.proposal.requested"
            && payload(fact).ok().is_some_and(|value| {
                value.get("turn_id").and_then(Value::as_str) == Some(planner_turn_id.as_str())
            })
    })?;
    let request_value = payload(request)?;
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
        fact.kind.as_str() == "plan.proposal.result_bound"
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
        {
            return Err(PlanProposalBindingError::CorruptState);
        }
        return result(bound, result_digest, response_items_json);
    }
    if watermark.session_version != expected_session_version {
        return Err(PlanProposalBindingError::ConcurrentModification);
    }
    let command_id = format!(
        "planner-bind-{}",
        &digest(format!("{}:{}", request.fact_id.as_str(), terminal.fact_id.as_str()).as_bytes())
            [..32]
    );
    let fact = FactDraft {
        fact_id: FactId::try_from(format!("fact-{}", digest(command_id.as_bytes())).as_str())
            .map_err(|_| PlanProposalBindingError::InvalidInput)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("plan.proposal.result_bound")
            .map_err(|_| PlanProposalBindingError::InvalidInput)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({
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
        }))
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
        result_digest: result_digest.into(),
        response_items_json: response_items_json.into(),
    })
}

fn result(
    fact: &DurableFact,
    digest: &str,
    json: &str,
) -> Result<BoundPlanProposalResult, PlanProposalBindingError> {
    Ok(BoundPlanProposalResult {
        binding_fact_id: fact.fact_id.as_str().into(),
        binding_position: fact.position,
        result_digest: digest.into(),
        response_items_json: json.into(),
    })
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
