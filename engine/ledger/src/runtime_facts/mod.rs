//! Strict C6 durable Runtime payload-v1 validation.

mod values;

use serde_json::Value;

use crate::{FactDraft, LedgerError};
use values::{enumeration, fields, non_empty, object, unsigned, EMPTY};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Whether a fact payload was applied as admitted v1 semantics or kept opaque.
pub enum RuntimeFactDisposition {
    /// Known C6 fact kind with a valid schema-v1 payload and envelope.
    AppliedV1,
    /// Unknown kind or newer schema retained only as an audit fact.
    Opaque,
}

/// Validates one admitted C6 payload and its required outer envelope identities.
pub fn validate_runtime_fact(fact: &FactDraft) -> Result<RuntimeFactDisposition, LedgerError> {
    if fact.kind.as_str() != "turn.cancel_requested" || fact.schema_version != 1 {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    if fact.turn_id.is_none()
        || fact.execution_id.is_some()
        || fact.model_request_id.is_some()
        || fact.tool_invocation_id.is_some()
    {
        return Err(LedgerError::InvalidFact);
    }
    let payload: Value =
        serde_json::from_str(fact.payload.as_json()).map_err(|_| LedgerError::InvalidFact)?;
    let payload = object(&payload)?;
    fields(
        payload,
        &["command_id", "reason", "requested_through_position"],
        EMPTY,
    )?;
    non_empty(payload, "command_id")?;
    enumeration(
        payload,
        "reason",
        &["user", "deadline", "shutdown", "operator", "policy"],
    )?;
    unsigned(payload, "requested_through_position", false)?;
    Ok(RuntimeFactDisposition::AppliedV1)
}
