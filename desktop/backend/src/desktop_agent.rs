//! Built-in Desktop Agent definition and its exact Runtime installation.

use std::collections::{BTreeMap, BTreeSet};

use garive_config::{
    resolve_definition, AgentDefinition, CapabilityKind, CapabilityReference,
    ContextPolicyCandidate, ContextPolicyReference, DefaultLimits, GovernancePolicy,
    GovernancePolicyCandidate, InteractionMode, ProductPolicy, ResolutionRegistry,
};
use garive_runtime::RuntimeAgentInstallation;

use crate::workspace_execution::{desktop_workspace_tool_definition, DESKTOP_WRITE_TOOL_REVISION};

/// Exact revision of the built-in Desktop Agent definition.
pub const DESKTOP_AGENT_REVISION: &str = "desktop.agent.v1";
const GOVERNANCE_ID: &str = "desktop.governance";
const GOVERNANCE_REVISION: &str = "desktop.governance.v1";
const CONTEXT_ID: &str = "desktop.context";
const CONTEXT_REVISION: &str = "desktop.context.v1";

/// Secret-free failure while resolving the built-in Desktop Agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DesktopAgentCompositionError;

/// Resolves the built-in Desktop Agent into one immutable Runtime installation.
pub fn builtin_desktop_agent_installation(
    definition_id: &str,
    instance_namespace: &str,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    let tool = desktop_workspace_tool_definition().map_err(|_| DesktopAgentCompositionError)?;
    let requirement_capabilities = BTreeSet::from(["filesystem_write".to_owned()]);
    let interaction_modes = BTreeSet::from([InteractionMode::Approval]);
    let limits = DefaultLimits::new(12, Some(131_072), Some(8_192), Some(600_000))
        .map_err(|_| DesktopAgentCompositionError)?;
    let definition = AgentDefinition::new(
        definition_id,
        DESKTOP_AGENT_REVISION,
        Vec::new(),
        Vec::new(),
        vec![CapabilityReference::new(
            CapabilityKind::Tool,
            tool.name(),
            DESKTOP_WRITE_TOOL_REVISION,
            3,
            true,
        )
        .map_err(|_| DesktopAgentCompositionError)?],
        GovernancePolicy::new(
            GOVERNANCE_ID,
            GOVERNANCE_REVISION,
            requirement_capabilities.clone(),
            interaction_modes.clone(),
        )
        .map_err(|_| DesktopAgentCompositionError)?,
        ContextPolicyReference::new(CONTEXT_ID, CONTEXT_REVISION)
            .map_err(|_| DesktopAgentCompositionError)?,
        limits.clone(),
        BTreeMap::from([("effective_snapshot".into(), 1)]),
    )
    .map_err(|_| DesktopAgentCompositionError)?;
    let snapshot = resolve_definition(
        &definition,
        &ResolutionRegistry {
            instructions: Vec::new(),
            model_roles: Vec::new(),
            tools: vec![tool],
            capability_descriptors: Vec::new(),
            governance_policies: vec![GovernancePolicyCandidate {
                policy_id: GOVERNANCE_ID.into(),
                exact_revision: GOVERNANCE_REVISION.into(),
                allowed_requirement_capabilities: requirement_capabilities.clone(),
                interaction_modes: interaction_modes.clone(),
            }],
            context_policies: vec![ContextPolicyCandidate {
                policy_id: CONTEXT_ID.into(),
                exact_revision: CONTEXT_REVISION.into(),
                descriptor_digest: "0".repeat(64),
            }],
            public_tool_activity_catalogue: None,
        },
        &ProductPolicy {
            allowed_requirement_capabilities: requirement_capabilities,
            interaction_modes,
            limit_caps: limits,
            admitted_contract_versions: BTreeMap::from([(
                "effective_snapshot".into(),
                BTreeSet::from([1]),
            )]),
        },
    )
    .map_err(|_| DesktopAgentCompositionError)?;
    RuntimeAgentInstallation::new(snapshot, instance_namespace, vec!["workspaces".into()])
        .map_err(|_| DesktopAgentCompositionError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_is_deterministic_and_contains_the_exact_workspace_tool() {
        let left = builtin_desktop_agent_installation("garive-work", "desktop-main").unwrap();
        let right = builtin_desktop_agent_installation("garive-work", "desktop-main").unwrap();
        assert_eq!(
            left.snapshot().snapshot_digest(),
            right.snapshot().snapshot_digest()
        );
        assert_eq!(left.tool_capabilities().definitions.len(), 1);
        assert_eq!(
            left.tool_capabilities().definitions[0].revision(),
            DESKTOP_WRITE_TOOL_REVISION
        );
        assert_eq!(left.installed_agent().runtime_limits.max_iterations, 12);
    }
}
