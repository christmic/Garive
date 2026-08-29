use garive_ledger::{ExecutionId, TurnSnapshot};
use serde_json::{Map, Value};

use super::types::{EffectiveRuntimeLimits, RuntimeCommandError, SuspendedTurnState};

/// Reconstructs a resumable Turn exclusively from one verified fixed Ledger prefix.
pub fn reconstruct_suspended_turn(
    snapshot: &TurnSnapshot,
) -> Result<SuspendedTurnState, RuntimeCommandError> {
    let turn_id = snapshot
        .facts
        .first()
        .and_then(|fact| fact.turn_id.clone())
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    let session_id = snapshot
        .facts
        .first()
        .map(|fact| fact.session_id.clone())
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    let started = snapshot
        .facts
        .iter()
        .find(|fact| fact.kind.as_str() == "turn.started")
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    let suspended = snapshot
        .facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == "turn.suspended")
        .ok_or(RuntimeCommandError::TurnNotResumable)?;
    if snapshot
        .facts
        .iter()
        .filter(|fact| fact.position > suspended.position)
        .any(|fact| {
            matches!(
                fact.kind.as_str(),
                "turn.started" | "turn.completed" | "turn.stopped" | "turn.failed"
            )
        })
    {
        return Err(RuntimeCommandError::TurnNotResumable);
    }
    let suspended_payload = payload(suspended)?;
    let execution_id = ExecutionId::try_from(text(&suspended_payload, "execution_id")?)
        .map_err(|_| RuntimeCommandError::CorruptLedger)?;
    let execution_start = snapshot
        .facts
        .iter()
        .rfind(|fact| {
            fact.kind.as_str() == "execution.started"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let execution_payload = payload(execution_start)?;
    let started_payload = payload(started)?;
    Ok(SuspendedTurnState {
        session_id,
        session_version: snapshot.session_version,
        turn_id,
        suspension_id: text(&suspended_payload, "suspension_id")?.to_owned(),
        agent_instance_id: identity(text(&started_payload, "agent_instance_id")?)?,
        definition_id: identity(text(&started_payload, "definition_id")?)?,
        definition_revision: identity(text(&started_payload, "definition_revision")?)?,
        snapshot_digest: text(&started_payload, "snapshot_digest")?.to_owned(),
        trusted_input_digest: text(&started_payload, "trusted_input_digest")?.to_owned(),
        through_position: snapshot.through_position,
        completed_iterations: unsigned(&execution_payload, "completed_iterations")?,
        recovery_ordinal: unsigned(&execution_payload, "recovery_ordinal")?,
        limits: limits(&execution_payload)?,
    })
}

fn limits(value: &Map<String, Value>) -> Result<EffectiveRuntimeLimits, RuntimeCommandError> {
    let limits = value
        .get("limits")
        .and_then(Value::as_object)
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    Ok(EffectiveRuntimeLimits {
        max_iterations: unsigned(limits, "max_iterations")?,
        max_input_tokens: optional_unsigned(limits, "max_input_tokens")?,
        max_output_tokens: optional_unsigned(limits, "max_output_tokens")?,
        deadline_budget_ms: optional_unsigned(limits, "deadline_budget_ms")?,
    })
}

fn payload(fact: &garive_ledger::DurableFact) -> Result<Map<String, Value>, RuntimeCommandError> {
    let value: Value = serde_json::from_str(fact.payload.as_json())
        .map_err(|_| RuntimeCommandError::CorruptLedger)?;
    value
        .as_object()
        .cloned()
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn optional_unsigned(
    value: &Map<String, Value>,
    key: &str,
) -> Result<Option<u64>, RuntimeCommandError> {
    value.get(key).map(|_| unsigned(value, key)).transpose()
}

fn identity<'a, T>(value: &'a str) -> Result<T, RuntimeCommandError>
where
    T: TryFrom<&'a str>,
{
    T::try_from(value).map_err(|_| RuntimeCommandError::CorruptLedger)
}
