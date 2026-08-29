use garive_ledger::{LedgerError, TurnSnapshot};
use serde_json::Value;

use crate::{SqliteLedger, SqliteLedgerError};

use super::{
    GetTurnQuery, RuntimeCommandError, RuntimeSuspensionKind, RuntimeSuspensionView,
    RuntimeTurnStatus, RuntimeTurnView,
};

/// Reconstructs a redacted Turn view from an optional fixed durable prefix.
pub fn get_turn(
    ledger: &SqliteLedger,
    query: &GetTurnQuery,
) -> Result<RuntimeTurnView, RuntimeCommandError> {
    let snapshot = ledger.load_turn(&query.turn_id).map_err(map_query_error)?;
    validate_owner(&snapshot, query)?;
    let through = query.through_position.unwrap_or(snapshot.through_position);
    if through == 0 || through > snapshot.through_position {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let facts: Vec<_> = snapshot
        .facts
        .iter()
        .filter(|fact| fact.position <= through)
        .collect();
    if facts.is_empty() || facts[0].kind.as_str() != "turn.started" {
        return Err(RuntimeCommandError::InvalidCommand);
    }

    let mut status = RuntimeTurnStatus::Open;
    let mut execution_id = None;
    let mut suspension = None;
    let mut completed_iterations = 0;
    let mut cancellation_requested = false;
    for fact in facts {
        match fact.kind.as_str() {
            "turn.started" => {
                status = RuntimeTurnStatus::Open;
                suspension = None;
            }
            "turn.suspended" => {
                let payload = payload(fact.payload.as_json())?;
                status = RuntimeTurnStatus::Suspended;
                suspension = Some(RuntimeSuspensionView {
                    suspension_id: text(&payload, "suspension_id")?.to_owned(),
                    kind: RuntimeSuspensionKind::parse(text(&payload, "reason")?)?,
                });
            }
            "turn.completed" => status = RuntimeTurnStatus::Completed,
            "turn.stopped" => status = RuntimeTurnStatus::Stopped,
            "turn.failed" => status = RuntimeTurnStatus::Failed,
            "turn.cancel_requested" => cancellation_requested = true,
            "execution.started" => execution_id = fact.execution_id.clone(),
            "execution.iteration_started" => {
                completed_iterations = completed_iterations
                    .max(unsigned(&payload(fact.payload.as_json())?, "iteration")?);
            }
            "execution.completed"
            | "execution.suspended"
            | "execution.stopped"
            | "execution.failed" => {
                completed_iterations = completed_iterations.max(unsigned(
                    &payload(fact.payload.as_json())?,
                    "completed_iterations",
                )?);
            }
            _ => {}
        }
    }
    if status != RuntimeTurnStatus::Suspended {
        suspension = None;
    }
    Ok(RuntimeTurnView {
        session_id: query.session_id.clone(),
        turn_id: query.turn_id.clone(),
        through_position: through,
        observed_session_version: snapshot.session_version,
        status,
        execution_id,
        suspension,
        completed_iterations,
        cancellation_requested,
    })
}

fn validate_owner(
    snapshot: &TurnSnapshot,
    query: &GetTurnQuery,
) -> Result<(), RuntimeCommandError> {
    let owner = snapshot
        .facts
        .first()
        .map(|fact| &fact.session_id)
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    if owner == &query.session_id {
        Ok(())
    } else {
        Err(RuntimeCommandError::InvalidCommand)
    }
}

fn payload(value: &str) -> Result<serde_json::Map<String, Value>, RuntimeCommandError> {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn text<'a>(
    value: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn unsigned(value: &serde_json::Map<String, Value>, key: &str) -> Result<u64, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn map_query_error(error: SqliteLedgerError) -> RuntimeCommandError {
    match error {
        SqliteLedgerError::Domain(LedgerError::MissingReference) => {
            RuntimeCommandError::InvalidCommand
        }
        SqliteLedgerError::Storage(_) => RuntimeCommandError::DurabilityFailure,
        _ => RuntimeCommandError::CorruptLedger,
    }
}
