//! Strict C6 durable Runtime payload-v1 validation.

mod artifact;
mod delegation;
mod effect;
mod knowledge;
mod memory;
mod model;
mod scheduler;
mod skill;
mod turn;
mod values;
mod workspace;

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
    let memory_family = kind.starts_with("memory.");
    let knowledge_family = kind.starts_with("knowledge.");
    let scheduler_family = kind.starts_with("schedule.");
    let delegation_family = kind.starts_with("delegation.");
    let workspace_family = kind.starts_with("workspace.");
    let artifact_family = kind.starts_with("artifact.");
    let memory_session_scoped = matches!(
        kind,
        "memory.tombstoned"
            | "memory.observation_recorded"
            | "memory.lifecycle_transitioned"
            | "memory.candidate_recorded"
            | "memory.maintenance_decided"
            | "memory.distillation_checkpointed"
            | "memory.audit_recorded"
            | "memory.promotion_requested"
            | "memory.promotion_recorded"
            | "memory.erasure_requested"
            | "memory.erasure_recorded"
    );
    let rejection = kind == "tool.preparation_rejected";
    if !kind.starts_with("turn.")
        && !execution_family
        && !model_family
        && !effect_family
        && !skill_family
        && !memory_family
        && !knowledge_family
        && !scheduler_family
        && !delegation_family
        && !workspace_family
        && !artifact_family
        && !rejection
    {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    if fact.schema_version != 1 {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    if fact.turn_id.is_some() != !(memory_session_scoped || scheduler_family || workspace_family)
        || fact.execution_id.is_some()
            != (execution_family
                || model_family
                || effect_family
                || skill_family
                || rejection
                || memory_family && !memory_session_scoped
                || knowledge_family
                || delegation_family
                || artifact_family)
        || fact.model_request_id.is_some() != (model_family || rejection)
        || fact.tool_invocation_id.is_some() != (effect_family || artifact_family)
    {
        return Err(LedgerError::InvalidFact);
    }
    let payload: Value =
        serde_json::from_str(fact.payload.as_json()).map_err(|_| LedgerError::InvalidFact)?;
    if artifact_family {
        artifact::validate(kind, object(&payload)?)?;
    } else if workspace_family {
        workspace::validate(kind, object(&payload)?)?;
    } else if delegation_family {
        delegation::validate(kind, object(&payload)?)?;
    } else if scheduler_family {
        scheduler::validate(kind, object(&payload)?)?;
    } else if knowledge_family {
        knowledge::validate(kind, object(&payload)?)?;
    } else if memory_family {
        memory::validate(kind, object(&payload)?)?;
    } else if skill_family {
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
