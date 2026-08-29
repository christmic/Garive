use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

const DEFINITION_CONTRACT: &str = "garive.skill-definition";
const CONTRACT_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_DESCRIPTION_BYTES: usize = 4_096;
const MAX_TAG_BYTES: usize = 128;
const MAX_REFERENCE_BYTES: usize = 256;
const SHA256_HEX_BYTES: usize = 64;

/// Stable S0 definition or activation failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkillErrorCode {
    /// A definition, identifier, tag, reference, or bound is invalid.
    InvalidSkill,
    /// The requested Skill is absent from the frozen snapshot.
    SkillNotEnabled,
    /// One Skill identity resolves to a different frozen revision.
    SkillRevisionMismatch,
    /// Exact instruction bytes do not match their content digest.
    InstructionDigestMismatch,
    /// The requested activation mode is outside S0 v1.
    ActivationModeUnsupported,
    /// A required capability is absent from the frozen snapshot.
    RequiredCapabilityUnavailable,
    /// Required instructions exceed an activation bound.
    InstructionLimitExceeded,
    /// One identity binds conflicting request or definition semantics.
    ActivationConflict,
    /// Runtime could not commit the activation fact.
    DurabilityFailure,
    /// Previously committed activation state is invalid.
    CorruptSkillState,
}

impl SkillErrorCode {
    /// Returns the stable portable failure name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidSkill => "invalid_skill",
            Self::SkillNotEnabled => "skill_not_enabled",
            Self::SkillRevisionMismatch => "skill_revision_mismatch",
            Self::InstructionDigestMismatch => "instruction_digest_mismatch",
            Self::ActivationModeUnsupported => "activation_mode_unsupported",
            Self::RequiredCapabilityUnavailable => "required_capability_unavailable",
            Self::InstructionLimitExceeded => "instruction_limit_exceeded",
            Self::ActivationConflict => "activation_conflict",
            Self::DurabilityFailure => "durability_failure",
            Self::CorruptSkillState => "corrupt_skill_state",
        }
    }
}

/// Typed S0 failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillError {
    code: SkillErrorCode,
}

impl SkillError {
    pub(crate) const fn new(code: SkillErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure classification.
    pub const fn code(&self) -> SkillErrorCode {
        self.code
    }
}

/// Exact UTF-8 instructions and their lowercase SHA-256 binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContentBinding {
    digest: String,
    inline_utf8: String,
}

impl ContentBinding {
    /// Validates exact inline content against its supplied digest.
    pub fn new(
        digest: impl Into<String>,
        inline_utf8: impl Into<String>,
    ) -> Result<Self, SkillError> {
        let value = Self {
            digest: digest.into(),
            inline_utf8: inline_utf8.into(),
        };
        if !valid_digest(&value.digest) || sha256(value.inline_utf8.as_bytes()) != value.digest {
            return Err(SkillError::new(SkillErrorCode::InstructionDigestMismatch));
        }
        Ok(value)
    }

    /// Constructs an inline binding and computes its exact digest.
    pub fn from_inline(inline_utf8: impl Into<String>) -> Self {
        let inline_utf8 = inline_utf8.into();
        Self {
            digest: sha256(inline_utf8.as_bytes()),
            inline_utf8,
        }
    }

    /// Returns the exact content digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
    /// Returns the exact instruction text without normalization.
    pub fn inline_utf8(&self) -> &str {
        &self.inline_utf8
    }
    /// Returns the exact UTF-8 byte length.
    pub fn byte_len(&self) -> u64 {
        self.inline_utf8.len() as u64
    }
}

/// Exact capability reference already admitted by D0.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CapabilityReference {
    kind: String,
    name: String,
    exact_revision: String,
    contract_version: String,
}

impl CapabilityReference {
    /// Validates a portable exact capability reference.
    pub fn new(
        kind: impl Into<String>,
        name: impl Into<String>,
        exact_revision: impl Into<String>,
        contract_version: impl Into<String>,
    ) -> Result<Self, SkillError> {
        let value = Self {
            kind: kind.into(),
            name: name.into(),
            exact_revision: exact_revision.into(),
            contract_version: contract_version.into(),
        };
        if [
            &value.kind,
            &value.name,
            &value.exact_revision,
            &value.contract_version,
        ]
        .into_iter()
        .any(|part| !valid_text(part, MAX_REFERENCE_BYTES))
        {
            return Err(SkillError::new(SkillErrorCode::InvalidSkill));
        }
        Ok(value)
    }
}

/// Exact tool reference that may only narrow the D0 tool catalog.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ExactToolReference {
    name: String,
    exact_revision: String,
}

impl ExactToolReference {
    /// Validates an exact tool name and revision.
    pub fn new(
        name: impl Into<String>,
        exact_revision: impl Into<String>,
    ) -> Result<Self, SkillError> {
        let value = Self {
            name: name.into(),
            exact_revision: exact_revision.into(),
        };
        if !valid_text(&value.name, MAX_REFERENCE_BYTES)
            || !valid_text(&value.exact_revision, MAX_REFERENCE_BYTES)
        {
            return Err(SkillError::new(SkillErrorCode::InvalidSkill));
        }
        Ok(value)
    }
}

/// Deterministic S0 v1 activation policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivationPolicy {
    /// Only a trusted explicit request may activate the Skill.
    ExplicitOnly,
    /// Trusted Runtime tags may activate the Skill.
    Tagged {
        /// Non-empty ordered unique normalized tags.
        tags: Vec<String>,
    },
}

impl ActivationPolicy {
    /// Constructs a tagged policy with ordered unique tags.
    pub fn tagged(tags: impl IntoIterator<Item = String>) -> Result<Self, SkillError> {
        let tags: Vec<_> = tags.into_iter().collect();
        if tags.is_empty()
            || tags.iter().any(|tag| !valid_text(tag, MAX_TAG_BYTES))
            || !ordered_unique(&tags)
        {
            return Err(SkillError::new(SkillErrorCode::InvalidSkill));
        }
        Ok(Self::Tagged { tags })
    }

    /// Counts exact intersections with a validated trusted tag set.
    pub fn matched_tag_count(&self, trusted: &BTreeSet<String>) -> usize {
        match self {
            Self::ExplicitOnly => 0,
            Self::Tagged { tags } => tags.iter().filter(|tag| trusted.contains(*tag)).count(),
        }
    }
}

/// Immutable instruction Skill admitted to one effective snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkillDefinition {
    skill_id: String,
    skill_revision: String,
    name: String,
    description: String,
    instructions: ContentBinding,
    activation: ActivationPolicy,
    required_capabilities: Vec<CapabilityReference>,
    allowed_tool_references: Vec<ExactToolReference>,
    max_instruction_bytes: u64,
    contract_version: String,
}

impl SkillDefinition {
    /// Validates every S0 definition field and exact ordered reference list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        skill_id: impl Into<String>,
        skill_revision: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        instructions: ContentBinding,
        activation: ActivationPolicy,
        required_capabilities: Vec<CapabilityReference>,
        allowed_tool_references: Vec<ExactToolReference>,
        max_instruction_bytes: u64,
        contract_version: impl Into<String>,
    ) -> Result<Self, SkillError> {
        let value = Self {
            skill_id: skill_id.into(),
            skill_revision: skill_revision.into(),
            name: name.into(),
            description: description.into(),
            instructions,
            activation,
            required_capabilities,
            allowed_tool_references,
            max_instruction_bytes,
            contract_version: contract_version.into(),
        };
        if !valid_text(&value.skill_id, MAX_ID_BYTES)
            || !valid_text(&value.skill_revision, MAX_ID_BYTES)
            || !valid_text(&value.name, MAX_NAME_BYTES)
            || !valid_text(&value.description, MAX_DESCRIPTION_BYTES)
            || !valid_text(&value.contract_version, MAX_REFERENCE_BYTES)
            || value.max_instruction_bytes == 0
            || value.instructions.byte_len() > value.max_instruction_bytes
            || !ordered_unique(&value.required_capabilities)
            || !ordered_unique(&value.allowed_tool_references)
        {
            return Err(SkillError::new(SkillErrorCode::InvalidSkill));
        }
        Ok(value)
    }

    /// Returns the stable Skill identity.
    pub fn skill_id(&self) -> &str {
        &self.skill_id
    }
    /// Returns the exact frozen revision.
    pub fn skill_revision(&self) -> &str {
        &self.skill_revision
    }
    /// Returns the exact instruction binding.
    pub const fn instructions(&self) -> &ContentBinding {
        &self.instructions
    }
    /// Returns the activation policy.
    pub const fn activation(&self) -> &ActivationPolicy {
        &self.activation
    }
    /// Returns exact required capabilities.
    pub fn required_capabilities(&self) -> &[CapabilityReference] {
        &self.required_capabilities
    }
    /// Returns exact tool references that narrow the snapshot.
    pub fn allowed_tool_references(&self) -> &[ExactToolReference] {
        &self.allowed_tool_references
    }

    /// Computes the RFC 8785 digest over the complete versioned definition.
    pub fn definition_digest(&self) -> Result<String, SkillError> {
        let preimage = json!({"contract": DEFINITION_CONTRACT, "version": CONTRACT_VERSION, "definition": self});
        let bytes = serde_jcs::to_vec(&preimage)
            .map_err(|_| SkillError::new(SkillErrorCode::InvalidSkill))?;
        Ok(sha256(&bytes))
    }
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && value.trim() == value
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn valid_digest(value: &str) -> bool {
    value.len() == SHA256_HEX_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
