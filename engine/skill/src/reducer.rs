use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{CapabilityReference, ExactToolReference, SkillDefinition, SkillError, SkillErrorCode};

const REQUEST_CONTRACT: &str = "garive.skill-activation";
const CONTRACT_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 128;
const MAX_TAG_BYTES: usize = 128;

/// Supported trusted activation mode in S0 v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    /// Activate one exact enabled Skill identity.
    Explicit,
    /// Match only trusted Runtime-supplied tags.
    Tagged,
}

impl ActivationMode {
    /// Parses the stable wire name and rejects future or semantic modes.
    pub fn from_wire(value: &str) -> Result<Self, SkillError> {
        match value {
            "explicit" => Ok(Self::Explicit),
            "tagged" => Ok(Self::Tagged),
            _ => Err(SkillError::new(SkillErrorCode::ActivationModeUnsupported)),
        }
    }
}

/// Durable reason one exact Skill entered the model context.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationReason {
    /// Selected by a trusted explicit request.
    Explicit,
    /// Selected by trusted tag intersection.
    TagMatch,
}

/// Validated activation request scoped to one Kernel iteration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkillActivationRequest {
    #[serde(skip)]
    activation_id: String,
    turn_id: String,
    execution_id: String,
    iteration: u64,
    mode: ActivationMode,
    requested_skill_id: Option<String>,
    trusted_tags: Vec<String>,
    through_position: u64,
    max_active_skills: u32,
    max_total_instruction_bytes: u64,
}

impl SkillActivationRequest {
    /// Validates a complete request; activation identity is excluded from its digest.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        activation_id: impl Into<String>,
        turn_id: impl Into<String>,
        execution_id: impl Into<String>,
        iteration: u64,
        mode: ActivationMode,
        requested_skill_id: Option<String>,
        trusted_tags: Vec<String>,
        through_position: u64,
        max_active_skills: u32,
        max_total_instruction_bytes: u64,
    ) -> Result<Self, SkillError> {
        let value = Self {
            activation_id: activation_id.into(),
            turn_id: turn_id.into(),
            execution_id: execution_id.into(),
            iteration,
            mode,
            requested_skill_id,
            trusted_tags,
            through_position,
            max_active_skills,
            max_total_instruction_bytes,
        };
        let explicit_shape = value.requested_skill_id.is_some() && value.trusted_tags.is_empty();
        let tagged_shape = value.requested_skill_id.is_none();
        if !valid_id(&value.activation_id)
            || !valid_id(&value.turn_id)
            || !valid_id(&value.execution_id)
            || value.iteration == 0
            || value.max_active_skills == 0
            || value.max_total_instruction_bytes == 0
            || value
                .requested_skill_id
                .as_deref()
                .is_some_and(|id| !valid_id(id))
            || value.trusted_tags.iter().any(|tag| !valid_tag(tag))
            || !ordered_unique(&value.trusted_tags)
            || (value.mode == ActivationMode::Explicit && !explicit_shape)
            || (value.mode == ActivationMode::Tagged && !tagged_shape)
        {
            return Err(SkillError::new(SkillErrorCode::InvalidSkill));
        }
        Ok(value)
    }

    /// Returns the outer idempotency identity.
    pub fn activation_id(&self) -> &str {
        &self.activation_id
    }
    /// Returns the fixed durable read position.
    pub const fn through_position(&self) -> u64 {
        self.through_position
    }
    /// Returns the portable activation mode.
    pub const fn mode(&self) -> ActivationMode {
        self.mode
    }

    /// Computes the exact S0 v1 request digest.
    pub fn request_digest(&self) -> Result<String, SkillError> {
        let value = serde_json::to_value(self)
            .map_err(|_| SkillError::new(SkillErrorCode::InvalidSkill))?;
        let preimage = json!({
            "contract": REQUEST_CONTRACT,
            "version": CONTRACT_VERSION,
            "request": value,
        });
        let bytes = serde_jcs::to_vec(&preimage)
            .map_err(|_| SkillError::new(SkillErrorCode::InvalidSkill))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

/// Exact activated Skill content and narrowed tool surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivatedSkill {
    skill_id: String,
    skill_revision: String,
    definition_digest: String,
    instructions: String,
    instruction_digest: String,
    reason: ActivationReason,
    allowed_tool_references: Vec<ExactToolReference>,
}

impl ActivatedSkill {
    /// Returns the stable Skill identity.
    pub fn skill_id(&self) -> &str {
        &self.skill_id
    }
    /// Returns the exact frozen revision.
    pub fn skill_revision(&self) -> &str {
        &self.skill_revision
    }
    /// Returns the complete definition digest.
    pub fn definition_digest(&self) -> &str {
        &self.definition_digest
    }
    /// Returns exact instruction text for C2 insertion.
    pub fn instructions(&self) -> &str {
        &self.instructions
    }
    /// Returns the exact instruction digest.
    pub fn instruction_digest(&self) -> &str {
        &self.instruction_digest
    }
    /// Returns the durable activation reason.
    pub const fn reason(&self) -> ActivationReason {
        self.reason
    }
    /// Returns the tool surface narrowed by this Skill.
    pub fn allowed_tool_references(&self) -> &[ExactToolReference] {
        &self.allowed_tool_references
    }
}

/// Deterministic S0 activation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SkillActivationResult {
    /// At least one exact Skill activated in canonical order.
    Activated {
        /// Ordered activated Skills.
        ordered_skills: Vec<ActivatedSkill>,
        /// Whether eligible tagged Skills were omitted by a bound.
        truncated: bool,
    },
    /// No trusted tags matched an enabled Skill.
    None,
}

/// Selects exact enabled Skills without I/O, authority, model calls, or mutation.
pub fn activate_skills(
    enabled: &[SkillDefinition],
    available_capabilities: &BTreeSet<CapabilityReference>,
    available_tools: &BTreeSet<ExactToolReference>,
    request: &SkillActivationRequest,
) -> Result<SkillActivationResult, SkillError> {
    let definitions = deduplicate(enabled)?;
    let trusted: BTreeSet<_> = request.trusted_tags.iter().cloned().collect();
    let mut candidates: Vec<(&SkillDefinition, usize, ActivationReason)> = match request.mode {
        ActivationMode::Explicit => {
            let requested = request
                .requested_skill_id
                .as_deref()
                .expect("validated explicit request");
            let matching: Vec<_> = definitions
                .values()
                .filter(|value| value.skill_id() == requested)
                .collect();
            match matching.as_slice() {
                [] => return Err(SkillError::new(SkillErrorCode::SkillNotEnabled)),
                [definition] => vec![(*definition, 0, ActivationReason::Explicit)],
                _ => return Err(SkillError::new(SkillErrorCode::SkillRevisionMismatch)),
            }
        }
        ActivationMode::Tagged => definitions
            .values()
            .filter_map(|definition| {
                let count = definition.activation().matched_tag_count(&trusted);
                (count > 0).then_some((*definition, count, ActivationReason::TagMatch))
            })
            .collect(),
    };
    candidates.sort_by_key(|(definition, count, _)| {
        (
            Reverse(*count),
            definition.skill_id(),
            definition.skill_revision(),
        )
    });

    let mut activated = Vec::new();
    let mut total_bytes = 0_u64;
    let mut truncated = false;
    for (definition, _, reason) in candidates {
        validate_snapshot_narrowing(definition, available_capabilities, available_tools)?;
        let next_bytes = total_bytes.saturating_add(definition.instructions().byte_len());
        if activated.len() == request.max_active_skills as usize
            || next_bytes > request.max_total_instruction_bytes
        {
            if request.mode == ActivationMode::Explicit {
                return Err(SkillError::new(SkillErrorCode::InstructionLimitExceeded));
            }
            truncated = true;
            continue;
        }
        total_bytes = next_bytes;
        activated.push(ActivatedSkill {
            skill_id: definition.skill_id().to_owned(),
            skill_revision: definition.skill_revision().to_owned(),
            definition_digest: definition.definition_digest()?,
            instructions: definition.instructions().inline_utf8().to_owned(),
            instruction_digest: definition.instructions().digest().to_owned(),
            reason,
            allowed_tool_references: definition.allowed_tool_references().to_vec(),
        });
    }
    if activated.is_empty() && !truncated {
        Ok(SkillActivationResult::None)
    } else {
        Ok(SkillActivationResult::Activated {
            ordered_skills: activated,
            truncated,
        })
    }
}

fn deduplicate(
    enabled: &[SkillDefinition],
) -> Result<BTreeMap<(String, String), &SkillDefinition>, SkillError> {
    let mut result = BTreeMap::new();
    for definition in enabled {
        let key = (
            definition.skill_id().to_owned(),
            definition.skill_revision().to_owned(),
        );
        if let Some(previous) = result.insert(key, definition) {
            if previous != definition {
                return Err(SkillError::new(SkillErrorCode::ActivationConflict));
            }
        }
    }
    Ok(result)
}

fn validate_snapshot_narrowing(
    definition: &SkillDefinition,
    capabilities: &BTreeSet<CapabilityReference>,
    tools: &BTreeSet<ExactToolReference>,
) -> Result<(), SkillError> {
    if definition
        .required_capabilities()
        .iter()
        .any(|reference| !capabilities.contains(reference))
    {
        return Err(SkillError::new(
            SkillErrorCode::RequiredCapabilityUnavailable,
        ));
    }
    if definition
        .allowed_tool_references()
        .iter()
        .any(|reference| !tools.contains(reference))
    {
        return Err(SkillError::new(SkillErrorCode::SkillNotEnabled));
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && value.trim() == value
}
fn valid_tag(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TAG_BYTES && value.trim() == value
}
fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
