use std::collections::BTreeSet;

use garive_ledger::{CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, TurnId};
use garive_skill::{
    activate_skills, ActivatedSkill, ActivationMode, ActivationReason, CapabilityReference,
    ExactToolReference, SkillActivationRequest, SkillActivationResult, SkillDefinition,
};
use serde_json::{json, Value};

use crate::RuntimeCommandError;

use super::encoding::digest;

/// Durable ownership and observation time for one S0 activation.
pub struct SkillActivationContext {
    /// Turn that owns the activation.
    pub turn_id: TurnId,
    /// Disposable execution that consumes the instructions.
    pub execution_id: ExecutionId,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Pure activation result paired with its commit-before-model fact.
pub struct PlannedSkillActivation {
    /// Exact activated instruction values supplied to Core after commit.
    pub activated_skills: Vec<ActivatedSkill>,
    /// Whether the canonical tagged candidate suffix was truncated.
    pub truncated: bool,
    /// Fact that must commit before Core can prepare a model request.
    pub fact: FactDraft,
}

/// Resolves one bounded S0 request and plans its exact durable fact.
pub fn plan_skill_activation(
    context: &SkillActivationContext,
    definitions: &[SkillDefinition],
    available_capabilities: &BTreeSet<CapabilityReference>,
    available_tools: &BTreeSet<ExactToolReference>,
    request: &SkillActivationRequest,
) -> Result<PlannedSkillActivation, RuntimeCommandError> {
    if chrono::DateTime::parse_from_rfc3339(&context.recorded_at).is_err() {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let request_digest = request
        .request_digest()
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let result = activate_skills(
        definitions,
        available_capabilities,
        available_tools,
        request,
    )
    .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let (activated_skills, truncated) = match result {
        SkillActivationResult::Activated {
            ordered_skills,
            truncated,
        } => (ordered_skills, truncated),
        SkillActivationResult::None => (Vec::new(), false),
    };
    let skills = activated_skills
        .iter()
        .map(|skill| {
            json!({
                "skill_id": skill.skill_id(),
                "skill_revision": skill.skill_revision(),
                "definition_digest": skill.definition_digest(),
                "instruction_digest": skill.instruction_digest(),
                "reason": reason(skill.reason()),
            })
        })
        .collect::<Vec<Value>>();
    let payload = json!({
        "activation_id": request.activation_id(),
        "request_digest": request_digest,
        "mode": mode(request.mode()),
        "through_position": request.through_position(),
        "skills": skills,
        "truncated": truncated,
    });
    let fact_digest = digest(format!("skill.activated:{}", request.activation_id()).as_bytes());
    let fact = FactDraft {
        fact_id: FactId::try_from(format!("fact-{fact_digest}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: Some(context.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("skill.activated").map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: context.recorded_at.clone(),
    };
    Ok(PlannedSkillActivation {
        activated_skills,
        truncated,
        fact,
    })
}

const fn mode(value: ActivationMode) -> &'static str {
    match value {
        ActivationMode::Explicit => "explicit",
        ActivationMode::Tagged => "tagged",
    }
}

const fn reason(value: ActivationReason) -> &'static str {
    match value {
        ActivationReason::Explicit => "explicit",
        ActivationReason::TagMatch => "tag_match",
    }
}
