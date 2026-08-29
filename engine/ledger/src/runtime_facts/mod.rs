//! Strict C6 durable Runtime payload-v1 validation.

mod effect;
mod model;
mod skill;
mod turn;
mod values;

use serde_json::Value;

use crate::{FactDraft, LedgerError};
use values::object;

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
    let kind = fact.kind.as_str();
    let execution_family = kind.starts_with("execution.");
    let model_family = kind.starts_with("model.");
    let effect_family = kind.starts_with("effect.") || kind.starts_with("interaction.");
    let skill_family = kind.starts_with("skill.");
    let rejection = kind == "tool.preparation_rejected";
    if !kind.starts_with("turn.")
        && !execution_family
        && !model_family
        && !effect_family
        && !skill_family
        && !rejection
    {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    if fact.schema_version != 1 {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    if fact.turn_id.is_none()
        || fact.execution_id.is_some()
            != (execution_family || model_family || effect_family || skill_family || rejection)
        || fact.model_request_id.is_some() != (model_family || rejection)
        || fact.tool_invocation_id.is_some() != effect_family
    {
        return Err(LedgerError::InvalidFact);
    }
    let payload: Value =
        serde_json::from_str(fact.payload.as_json()).map_err(|_| LedgerError::InvalidFact)?;
    if skill_family {
        skill::validate(kind, object(&payload)?)?;
    } else if effect_family || rejection {
        effect::validate(kind, object(&payload)?)?;
    } else if model_family {
        model::validate(kind, object(&payload)?)?;
    } else {
        turn::validate(kind, object(&payload)?)?;
    }
    Ok(RuntimeFactDisposition::AppliedV1)
}
