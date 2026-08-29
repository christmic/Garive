//! Complete Agent Definition aggregate and cross-field invariants.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::definition::{
    require_text, CapabilityReference, ContextPolicyReference, DefaultLimits, GovernancePolicy,
    InstructionReference, ModelRoleRequirement, ResolutionError, ResolutionErrorCode,
};

/// Complete immutable portable Agent intent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentDefinition {
    /// Stable definition identity.
    pub(crate) definition_id: String,
    /// Exact immutable definition revision.
    pub(crate) revision: String,
    /// Ordered instruction roots from low to high precedence.
    pub(crate) instruction_sources: Vec<InstructionReference>,
    /// Ordered neutral model roles.
    pub(crate) model_roles: Vec<ModelRoleRequirement>,
    /// Exact capability references.
    pub(crate) capabilities: Vec<CapabilityReference>,
    /// Requested governance policy.
    pub(crate) governance: GovernancePolicy,
    /// Exact context policy reference.
    pub(crate) context_policy: ContextPolicyReference,
    /// Default execution limits.
    pub(crate) limits: DefaultLimits,
    /// Required named portable contract versions.
    pub(crate) contract_versions: BTreeMap<String, u64>,
}

impl AgentDefinition {
    /// Validates all definition-local identity and uniqueness invariants.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        definition_id: impl Into<String>,
        revision: impl Into<String>,
        instruction_sources: Vec<InstructionReference>,
        model_roles: Vec<ModelRoleRequirement>,
        capabilities: Vec<CapabilityReference>,
        governance: GovernancePolicy,
        context_policy: ContextPolicyReference,
        limits: DefaultLimits,
        contract_versions: BTreeMap<String, u64>,
    ) -> Result<Self, ResolutionError> {
        let definition_id = definition_id.into();
        let revision = revision.into();
        require_text(&definition_id, "/definition_id")?;
        require_text(&revision, "/revision")?;
        let instruction_keys: BTreeSet<_> = instruction_sources
            .iter()
            .map(|item| item.source_id.as_str())
            .collect();
        let role_keys: BTreeSet<_> = model_roles
            .iter()
            .map(|item| item.role_id.as_str())
            .collect();
        let capability_keys: BTreeSet<_> = capabilities
            .iter()
            .map(|item| (item.kind, item.name.as_str()))
            .collect();
        if instruction_keys.len() != instruction_sources.len() {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/instruction_sources",
            ));
        }
        if role_keys.len() != model_roles.len() {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/model_roles",
            ));
        }
        if capability_keys.len() != capabilities.len() {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/capabilities",
            ));
        }
        if contract_versions
            .iter()
            .any(|(name, version)| name.is_empty() || *version == 0)
        {
            return Err(ResolutionError::new(
                ResolutionErrorCode::InvalidDefinition,
                "/contract_versions",
            ));
        }
        Ok(Self {
            definition_id,
            revision,
            instruction_sources,
            model_roles,
            capabilities,
            governance,
            context_policy,
            limits,
            contract_versions,
        })
    }

    /// Returns the stable definition identity.
    pub fn definition_id(&self) -> &str {
        &self.definition_id
    }

    /// Returns the exact immutable revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Returns ordered instruction roots.
    pub fn instruction_sources(&self) -> &[InstructionReference] {
        &self.instruction_sources
    }

    /// Returns ordered neutral model roles.
    pub fn model_roles(&self) -> &[ModelRoleRequirement] {
        &self.model_roles
    }

    /// Returns exact capability references.
    pub fn capabilities(&self) -> &[CapabilityReference] {
        &self.capabilities
    }

    /// Returns requested governance.
    pub const fn governance(&self) -> &GovernancePolicy {
        &self.governance
    }

    /// Returns the exact context policy reference.
    pub const fn context_policy(&self) -> &ContextPolicyReference {
        &self.context_policy
    }

    /// Returns default execution limits.
    pub const fn limits(&self) -> &DefaultLimits {
        &self.limits
    }

    /// Returns required portable contract versions.
    pub const fn contract_versions(&self) -> &BTreeMap<String, u64> {
        &self.contract_versions
    }
}
