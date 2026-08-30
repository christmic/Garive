//! Strict C6 durable Runtime payload-v1 validation.

mod artifact;
mod delegation;
mod effect;
mod f0;
mod goal;
mod knowledge;
mod memory;
mod model;
mod plan;
mod plan_step;
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
    /// Known additive C5b fact kind with a valid schema-v2 payload and envelope.
    AppliedV2,
    /// Known additive F0 Prepared Call with valid schema-v3 payload and envelope.
    AppliedV3,
    /// Unknown kind or newer schema retained only as an audit fact.
    Opaque,
}

/// Validates one admitted C6 payload and its required outer envelope identities.
pub fn validate_runtime_fact(fact: &FactDraft) -> Result<RuntimeFactDisposition, LedgerError> {
    let kind = fact.kind.as_str();
    let execution_family = kind.starts_with("execution.");
    let model_family = kind.starts_with("model.");
    let effect_family = kind.starts_with("effect.") || kind.starts_with("interaction.");
    let f0_family = kind.starts_with("safety.") || kind.starts_with("sandbox.");
    let skill_family = kind.starts_with("skill.");
    let memory_family = kind.starts_with("memory.");
    let knowledge_family = kind.starts_with("knowledge.");
    let scheduler_family = kind.starts_with("schedule.");
    let delegation_family = kind.starts_with("delegation.");
    let workspace_family = kind.starts_with("workspace.");
    let artifact_family = kind.starts_with("artifact.");
    let goal_family = kind.starts_with("goal.");
    let plan_family = kind.starts_with("plan.");
    let memory_session_scoped = matches!(
        kind,
        "memory.tombstoned"
            | "memory.revision_classified"
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
        && !f0_family
        && !skill_family
        && !memory_family
        && !knowledge_family
        && !scheduler_family
        && !delegation_family
        && !workspace_family
        && !artifact_family
        && !goal_family
        && !plan_family
        && !rejection
    {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    let effect_prepared_v2 = kind == "effect.prepared" && fact.schema_version == 2;
    let effect_prepared_v3 = kind == "effect.prepared" && fact.schema_version == 3;
    let effect_authorized_v2 = kind == "effect.authorized" && fact.schema_version == 2;
    if fact.schema_version != 1
        && !effect_prepared_v2
        && !effect_prepared_v3
        && !effect_authorized_v2
    {
        return Ok(RuntimeFactDisposition::Opaque);
    }
    if fact.turn_id.is_some()
        != !(memory_session_scoped
            || scheduler_family
            || workspace_family
            || goal_family
            || plan_family)
        || fact.execution_id.is_some()
            != (execution_family
                || model_family
                || effect_family
                || f0_family
                || skill_family
                || rejection
                || memory_family && !memory_session_scoped
                || knowledge_family
                || delegation_family
                || artifact_family)
        || fact.model_request_id.is_some() != (model_family || rejection)
        || fact.tool_invocation_id.is_some() != (effect_family || f0_family || artifact_family)
    {
        return Err(LedgerError::InvalidFact);
    }
    let payload: Value =
        serde_json::from_str(fact.payload.as_json()).map_err(|_| LedgerError::InvalidFact)?;
    if artifact_family {
        artifact::validate(kind, object(&payload)?)?;
    } else if goal_family {
        goal::validate(kind, object(&payload)?)?;
    } else if plan_family {
        plan::validate(kind, object(&payload)?)?;
    } else if workspace_family {
        workspace::validate(kind, object(&payload)?)?;
    } else if effect_prepared_v2 {
        effect::validate_prepared_v2(object(&payload)?)?;
    } else if effect_prepared_v3 {
        effect::validate_prepared_v3(object(&payload)?)?;
    } else if effect_authorized_v2 {
        effect::validate_authorized_v2(object(&payload)?)?;
    } else if f0_family {
        f0::validate(kind, object(&payload)?)?;
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
    Ok(if effect_prepared_v2 || effect_authorized_v2 {
        RuntimeFactDisposition::AppliedV2
    } else if effect_prepared_v3 {
        RuntimeFactDisposition::AppliedV3
    } else {
        RuntimeFactDisposition::AppliedV1
    })
}
