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
    pub definition_id: String,
    /// Exact immutable definition revision.
    pub revision: String,
    /// Ordered instruction roots from low to high precedence.
    pub instruction_sources: Vec<InstructionReference>,
    /// Ordered neutral model roles.
    pub model_roles: Vec<ModelRoleRequirement>,
    /// Exact capability references.
    pub capabilities: Vec<CapabilityReference>,
    /// Requested governance policy.
    pub governance: GovernancePolicy,
    /// Exact context policy reference.
    pub context_policy: ContextPolicyReference,
    /// Default execution limits.
    pub limits: DefaultLimits,
    /// Required named portable contract versions.
    pub contract_versions: BTreeMap<String, u64>,
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
}
