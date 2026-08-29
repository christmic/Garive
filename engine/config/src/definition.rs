//! Immutable Agent Definition values and local invariant validation.

use std::collections::BTreeSet;

use serde::Serialize;

/// Stable D0 definition or resolution failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionErrorCode {
    /// Exact definition identity is absent.
    DefinitionNotFound,
    /// A required exact reference is absent.
    ReferenceNotFound,
    /// More than one candidate matches an exact reference.
    ReferenceAmbiguous,
    /// Exact-reference expansion contains a cycle.
    ReferenceCycle,
    /// A required contract version is not admitted.
    UnsupportedContractVersion,
    /// Product authority cannot satisfy the definition.
    PolicyIncompatible,
    /// A definition or cross-field invariant is invalid.
    InvalidDefinition,
    /// A digest input cannot satisfy canonicalization.
    NonCanonicalValue,
}

/// Secret-free D0 failure with a stable JSON-pointer path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionError {
    code: ResolutionErrorCode,
    path: String,
}

impl ResolutionError {
    pub(crate) fn new(code: ResolutionErrorCode, path: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
        }
    }

    /// Returns the stable failure code.
    pub const fn code(&self) -> ResolutionErrorCode {
        self.code
    }

    /// Returns the secret-free path into the rejected input.
    pub fn path(&self) -> &str {
        &self.path
    }
}

pub(crate) fn require_text(value: &str, path: &str) -> Result<(), ResolutionError> {
    if value.is_empty() {
        Err(ResolutionError::new(
            ResolutionErrorCode::InvalidDefinition,
            path,
        ))
    } else {
        Ok(())
    }
}

/// Exact instruction source reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstructionReference {
    /// Stable source identity.
    pub source_id: String,
    /// Exact immutable source revision.
    pub exact_revision: String,
    /// Whether absence must fail resolution.
    pub required: bool,
}

impl InstructionReference {
    /// Validates one exact instruction reference.
    pub fn new(
        source_id: impl Into<String>,
        exact_revision: impl Into<String>,
        required: bool,
    ) -> Result<Self, ResolutionError> {
        let value = Self {
            source_id: source_id.into(),
            exact_revision: exact_revision.into(),
            required,
        };
        require_text(&value.source_id, "/instruction_sources/source_id")?;
        require_text(&value.exact_revision, "/instruction_sources/exact_revision")?;
        Ok(value)
    }
}

/// Neutral model role requirement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ModelRoleRequirement {
    /// Definition-local role identity.
    pub role_id: String,
    /// Canonically sorted unique capabilities required from the target.
    pub required_capabilities: BTreeSet<String>,
    /// Whether absence or incompatibility must fail resolution.
    pub required: bool,
}

impl ModelRoleRequirement {
    /// Validates one model role and rejects duplicate capabilities.
    pub fn new(
        role_id: impl Into<String>,
        capabilities: impl IntoIterator<Item = String>,
        required: bool,
    ) -> Result<Self, ResolutionError> {
        let role_id = role_id.into();
        require_text(&role_id, "/model_roles/role_id")?;
        let values: Vec<_> = capabilities.into_iter().collect();
        let required_capabilities: BTreeSet<_> = values.iter().cloned().collect();
        if values.iter().any(String::is_empty) || required_capabilities.len() != values.len() {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/model_roles/required_capabilities",
            ));
        }
        Ok(Self {
            role_id,
            required_capabilities,
            required,
        })
    }
}

/// Portable capability kind admitted by D0.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    /// C4 tool definition.
    Tool,
    /// Skill descriptor.
    Skill,
    /// Memory descriptor.
    Memory,
    /// Knowledge descriptor.
    Knowledge,
    /// Delegation descriptor.
    Delegation,
}

/// Exact capability reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityReference {
    /// Capability family.
    pub kind: CapabilityKind,
    /// Stable capability name.
    pub name: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Required portable contract version.
    pub contract_version: u64,
    /// Whether absence or policy rejection must fail resolution.
    pub required: bool,
}

impl CapabilityReference {
    /// Validates one exact capability reference.
    pub fn new(
        kind: CapabilityKind,
        name: impl Into<String>,
        exact_revision: impl Into<String>,
        contract_version: u64,
        required: bool,
    ) -> Result<Self, ResolutionError> {
        let value = Self {
            kind,
            name: name.into(),
            exact_revision: exact_revision.into(),
            contract_version,
            required,
        };
        require_text(&value.name, "/capabilities/name")?;
        require_text(&value.exact_revision, "/capabilities/exact_revision")?;
        if contract_version == 0 {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/capabilities/contract_version",
            ));
        }
        Ok(value)
    }
}

/// Interaction mode that effective governance may admit.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    /// Human or product authority approval.
    Approval,
    /// Typed external input request.
    ExternalInput,
}

/// Required unmatched-policy behavior in D0 v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultUnmatched {
    /// Reject an unmatched request.
    Deny,
}

/// Requested governance policy and portable authority surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GovernancePolicy {
    /// Exact policy identity.
    pub policy_id: String,
    /// Exact policy revision.
    pub exact_revision: String,
    /// Canonically sorted executor capabilities the definition may request.
    pub allowed_requirement_capabilities: BTreeSet<String>,
    /// Canonically ordered interaction modes.
    pub interaction_modes: BTreeSet<InteractionMode>,
    /// Fail-closed unmatched behavior.
    pub default_unmatched: DefaultUnmatched,
}

impl GovernancePolicy {
    /// Validates policy identity and unique requested sets.
    pub fn new(
        policy_id: impl Into<String>,
        exact_revision: impl Into<String>,
        capabilities: impl IntoIterator<Item = String>,
        modes: impl IntoIterator<Item = InteractionMode>,
    ) -> Result<Self, ResolutionError> {
        let policy_id = policy_id.into();
        let exact_revision = exact_revision.into();
        require_text(&policy_id, "/governance/policy_id")?;
        require_text(&exact_revision, "/governance/exact_revision")?;
        let capability_values: Vec<_> = capabilities.into_iter().collect();
        let allowed_requirement_capabilities: BTreeSet<_> =
            capability_values.iter().cloned().collect();
        let mode_values: Vec<_> = modes.into_iter().collect();
        let interaction_modes: BTreeSet<_> = mode_values.iter().copied().collect();
        if capability_values.iter().any(String::is_empty)
            || allowed_requirement_capabilities.len() != capability_values.len()
            || interaction_modes.len() != mode_values.len()
        {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/governance",
            ));
        }
        Ok(Self {
            policy_id,
            exact_revision,
            allowed_requirement_capabilities,
            interaction_modes,
            default_unmatched: DefaultUnmatched::Deny,
        })
    }
}

/// Exact context policy reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContextPolicyReference {
    /// Stable policy identity.
    pub policy_id: String,
    /// Exact immutable revision.
    pub exact_revision: String,
}

impl ContextPolicyReference {
    /// Validates an exact context policy reference.
    pub fn new(
        policy_id: impl Into<String>,
        exact_revision: impl Into<String>,
    ) -> Result<Self, ResolutionError> {
        let value = Self {
            policy_id: policy_id.into(),
            exact_revision: exact_revision.into(),
        };
        require_text(&value.policy_id, "/context_policy/policy_id")?;
        require_text(&value.exact_revision, "/context_policy/exact_revision")?;
        Ok(value)
    }
}

/// Definition defaults that Runtime may only tighten.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DefaultLimits {
    /// Maximum completed Kernel iterations.
    pub max_iterations: u64,
    /// Optional maximum model input tokens.
    pub max_input_tokens: Option<u64>,
    /// Optional maximum model output tokens.
    pub max_output_tokens: Option<u64>,
    /// Optional wall-clock budget in milliseconds.
    pub deadline_budget_ms: Option<u64>,
}

impl DefaultLimits {
    /// Rejects zero values while retaining explicit optional bounds.
    pub fn new(
        max_iterations: u64,
        max_input_tokens: Option<u64>,
        max_output_tokens: Option<u64>,
        deadline_budget_ms: Option<u64>,
    ) -> Result<Self, ResolutionError> {
        if max_iterations == 0
            || [max_input_tokens, max_output_tokens, deadline_budget_ms]
                .into_iter()
                .flatten()
                .any(|value| value == 0)
        {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/limits",
            ));
        }
        Ok(Self {
            max_iterations,
            max_input_tokens,
            max_output_tokens,
            deadline_budget_ms,
        })
    }
}
