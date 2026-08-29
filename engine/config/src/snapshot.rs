//! Runtime-supplied exact candidates and immutable effective snapshot values.

use std::collections::{BTreeMap, BTreeSet};

use garive_tools::ToolDefinition;
use serde::Serialize;

use crate::{
    CapabilityKind, DefaultLimits, DefaultUnmatched, InstructionReference, InteractionMode,
    ResolutionError, ResolutionErrorCode,
};

/// Exact instruction registry resource with ordered dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionResource {
    /// Stable source identity.
    pub source_id: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Exact UTF-8 instruction content.
    pub content_utf8: String,
    /// Ordered exact dependencies expanded before this resource.
    pub dependencies: Vec<InstructionReference>,
}

/// Neutral model target candidate supplied by Runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRoleCandidate {
    /// Definition-local role identity.
    pub role_id: String,
    /// Stable neutral target identity.
    pub capability_target_id: String,
    /// Canonically sorted target capability set.
    pub admitted_capabilities: BTreeSet<String>,
}

/// Exact non-tool capability descriptor supplied by Runtime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityDescriptor {
    /// Capability family.
    pub kind: CapabilityKind,
    /// Stable capability name.
    pub name: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Portable contract version.
    pub contract_version: u64,
    /// Digest of public executable descriptor meaning.
    pub descriptor_digest: String,
}

/// Exact governance registry candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernancePolicyCandidate {
    /// Stable policy identity.
    pub policy_id: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Executor capability ceiling declared by the policy.
    pub allowed_requirement_capabilities: BTreeSet<String>,
    /// Interaction-mode ceiling declared by the policy.
    pub interaction_modes: BTreeSet<InteractionMode>,
}

/// Exact context policy registry candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPolicyCandidate {
    /// Stable policy identity.
    pub policy_id: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Digest of public context policy meaning.
    pub descriptor_digest: String,
}

/// Frozen Runtime registry view used for one resolution attempt.
#[derive(Clone, Debug)]
pub struct ResolutionRegistry {
    /// Instruction candidates; duplicate exact keys are ambiguity evidence.
    pub instructions: Vec<InstructionResource>,
    /// Model target candidates; duplicate role IDs are ambiguity evidence.
    pub model_roles: Vec<ModelRoleCandidate>,
    /// Exact C4 tool candidates.
    pub tools: Vec<ToolDefinition>,
    /// Exact non-tool capability candidates.
    pub capability_descriptors: Vec<CapabilityDescriptor>,
    /// Exact governance candidates.
    pub governance_policies: Vec<GovernancePolicyCandidate>,
    /// Exact context policy candidates.
    pub context_policies: Vec<ContextPolicyCandidate>,
}

/// Product and actor ceilings applied without mutating the definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductPolicy {
    /// Executor capabilities product authority can grant.
    pub allowed_requirement_capabilities: BTreeSet<String>,
    /// Interaction modes product authority supports.
    pub interaction_modes: BTreeSet<InteractionMode>,
    /// Equal or stricter external limits.
    pub limit_caps: DefaultLimits,
    /// Named portable contract versions admitted by the product.
    pub admitted_contract_versions: BTreeMap<String, BTreeSet<u64>>,
}

/// Resolved exact instruction included in execution precedence order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedInstruction {
    /// Stable source identity.
    pub source_id: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Exact UTF-8 content.
    pub content_utf8: String,
    /// Lowercase SHA-256 over exact content bytes.
    pub content_digest: String,
}

/// Resolved neutral model role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedModelRole {
    /// Definition-local role identity.
    pub role_id: String,
    /// Stable neutral target identity.
    pub capability_target_id: String,
    /// Canonically sorted capabilities admitted for the role.
    pub admitted_capabilities: BTreeSet<String>,
}

/// Exact enabled tool definitions and other public capability descriptors.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EffectiveCapabilitySnapshot {
    /// Full exact C4 definitions.
    pub tools: Vec<ToolDefinition>,
    /// Exact non-tool descriptors.
    pub descriptors: Vec<CapabilityDescriptor>,
}

/// Governance policy after intersection with Runtime authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectiveGovernancePolicy {
    /// Exact definition-requested policy identity.
    pub policy_id: String,
    /// Exact policy revision.
    pub exact_revision: String,
    /// Effective executor capability ceiling.
    pub allowed_requirement_capabilities: BTreeSet<String>,
    /// Effective interaction modes.
    pub interaction_modes: BTreeSet<InteractionMode>,
    /// Fail-closed unmatched behavior.
    pub default_unmatched: DefaultUnmatched,
}

/// Resolved exact context policy descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedContextPolicy {
    /// Stable policy identity.
    pub policy_id: String,
    /// Exact immutable revision.
    pub exact_revision: String,
    /// Digest of public policy meaning.
    pub descriptor_digest: String,
}

/// Effective limits after monotonic Runtime tightening.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EffectiveLimits {
    /// Maximum completed Kernel iterations.
    pub max_iterations: u64,
    /// Optional maximum model input tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    /// Optional maximum model output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// Optional wall-clock budget in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_budget_ms: Option<u64>,
}

/// Deeply immutable exact execution meaning bound to one durable Turn.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EffectiveAgentSnapshot {
    /// Stable definition identity.
    pub(crate) definition_id: String,
    /// Exact definition revision.
    pub(crate) definition_revision: String,
    /// Lowercase SHA-256 of the canonical definition envelope.
    pub(crate) definition_digest: String,
    /// Ordered resolved instruction content.
    pub(crate) instructions: Vec<ResolvedInstruction>,
    /// Ordered resolved neutral model roles.
    pub(crate) model_roles: Vec<ResolvedModelRole>,
    /// Exact enabled capabilities.
    pub(crate) capabilities: EffectiveCapabilitySnapshot,
    /// Effective fail-closed governance.
    pub(crate) governance: EffectiveGovernancePolicy,
    /// Exact resolved context policy.
    pub(crate) context_policy: ResolvedContextPolicy,
    /// Effective bounded limits.
    pub(crate) limits: EffectiveLimits,
    /// Required portable contract versions.
    pub(crate) contract_versions: BTreeMap<String, u64>,
    /// Lowercase SHA-256 of the canonical snapshot preimage.
    pub(crate) snapshot_digest: String,
}

impl EffectiveAgentSnapshot {
    /// Returns the stable definition identity.
    pub fn definition_id(&self) -> &str {
        &self.definition_id
    }

    /// Returns the exact definition revision.
    pub fn definition_revision(&self) -> &str {
        &self.definition_revision
    }

    /// Returns the canonical definition digest.
    pub fn definition_digest(&self) -> &str {
        &self.definition_digest
    }

    /// Returns ordered resolved instructions.
    pub fn instructions(&self) -> &[ResolvedInstruction] {
        &self.instructions
    }

    /// Returns ordered resolved model roles.
    pub fn model_roles(&self) -> &[ResolvedModelRole] {
        &self.model_roles
    }

    /// Returns the exact enabled capability snapshot.
    pub const fn capabilities(&self) -> &EffectiveCapabilitySnapshot {
        &self.capabilities
    }

    /// Returns effective governance.
    pub const fn governance(&self) -> &EffectiveGovernancePolicy {
        &self.governance
    }

    /// Returns the exact resolved context policy.
    pub const fn context_policy(&self) -> &ResolvedContextPolicy {
        &self.context_policy
    }

    /// Returns effective bounded limits.
    pub const fn limits(&self) -> &EffectiveLimits {
        &self.limits
    }

    /// Returns required portable contract versions.
    pub const fn contract_versions(&self) -> &BTreeMap<String, u64> {
        &self.contract_versions
    }

    /// Returns the canonical snapshot digest.
    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }

    /// Validates that continuation reuses the exact durable binding.
    pub fn validate_continuation(
        &self,
        definition_revision: &str,
        snapshot_digest: &str,
    ) -> Result<(), ResolutionError> {
        if self.definition_revision != definition_revision
            || self.snapshot_digest != snapshot_digest
        {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/continuation_binding",
            ));
        }
        Ok(())
    }
}
