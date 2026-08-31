//! Built-in Desktop Agent definition and its exact Runtime installation.

use std::collections::{BTreeMap, BTreeSet};

use garive_config::{
    resolve_definition, AgentDefinition, CapabilityKind, CapabilityReference,
    ContextPolicyCandidate, ContextPolicyReference, DefaultLimits, GovernancePolicy,
    GovernancePolicyCandidate, InteractionMode, ProductPolicy, ResolutionRegistry,
};
use garive_core::AgentToolCapabilities;
use garive_runtime::RuntimeAgentInstallation;
use garive_tools::{
    ToolDefinition, T1_APPLY_PATCH, T1_LIST, T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT,
    T1_TOOL_REVISION,
};

use crate::workspace_execution::desktop_workspace_tool_definition;

/// Exact revision of the built-in Desktop Agent definition.
pub const DESKTOP_AGENT_REVISION: &str = "desktop.agent.v1";
/// Exact revision of the built-in T1 Workspace Agent definition.
pub const DESKTOP_WORKSPACE_AGENT_REVISION: &str = "desktop.workspace-agent.v1";
const GOVERNANCE_ID: &str = "desktop.governance";
const GOVERNANCE_REVISION: &str = "desktop.governance.v1";
const WORKSPACE_GOVERNANCE_ID: &str = "desktop.workspace-governance";
const WORKSPACE_GOVERNANCE_REVISION: &str = "desktop.workspace-governance.v1";
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
    resolve_desktop_installation(
        definition_id,
        DESKTOP_AGENT_REVISION,
        instance_namespace,
        vec![tool],
        GOVERNANCE_ID,
        GOVERNANCE_REVISION,
        BTreeSet::from(["filesystem_write".to_owned()]),
    )
}

/// Resolves the exact six-tool T1 Workspace Agent from machine capabilities.
pub fn builtin_desktop_workspace_agent_installation(
    definition_id: &str,
    instance_namespace: &str,
    t1: &AgentToolCapabilities,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    let expected = BTreeSet::from([
        T1_APPLY_PATCH,
        T1_LIST,
        T1_PROCESS_RUN,
        T1_READ_TEXT,
        T1_SEARCH_TEXT,
    ]);
    if t1.definitions.len() != expected.len()
        || t1
            .definitions
            .iter()
            .map(|definition| definition.name())
            .collect::<BTreeSet<_>>()
            != expected
        || t1
            .definitions
            .iter()
            .any(|definition| definition.revision() != T1_TOOL_REVISION)
    {
        return Err(DesktopAgentCompositionError);
    }
    let mut tools = t1.definitions.clone();
    tools.push(desktop_workspace_tool_definition().map_err(|_| DesktopAgentCompositionError)?);
    tools.sort_by(|left, right| left.name().cmp(right.name()));
    resolve_desktop_installation(
        definition_id,
        DESKTOP_WORKSPACE_AGENT_REVISION,
        instance_namespace,
        tools,
        WORKSPACE_GOVERNANCE_ID,
        WORKSPACE_GOVERNANCE_REVISION,
        BTreeSet::from([
            "filesystem_read".to_owned(),
            "filesystem_write".to_owned(),
            "process".to_owned(),
        ]),
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_desktop_installation(
    definition_id: &str,
    definition_revision: &str,
    instance_namespace: &str,
    tools: Vec<ToolDefinition>,
    governance_id: &str,
    governance_revision: &str,
    requirement_capabilities: BTreeSet<String>,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    let interaction_modes = BTreeSet::from([InteractionMode::Approval]);
    let limits = DefaultLimits::new(12, Some(131_072), Some(8_192), Some(600_000))
        .map_err(|_| DesktopAgentCompositionError)?;
    let definition = AgentDefinition::new(
        definition_id,
        definition_revision,
        Vec::new(),
        Vec::new(),
        tools
            .iter()
            .map(|tool| {
                CapabilityReference::new(
                    CapabilityKind::Tool,
                    tool.name(),
                    tool.revision(),
                    3,
                    true,
                )
                .map_err(|_| DesktopAgentCompositionError)
            })
            .collect::<Result<Vec<_>, _>>()?,
        GovernancePolicy::new(
            governance_id,
            governance_revision,
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
            tools,
            capability_descriptors: Vec::new(),
            governance_policies: vec![GovernancePolicyCandidate {
                policy_id: governance_id.into(),
                exact_revision: governance_revision.into(),
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
    use garive_tools::BuiltinT1Catalogue;

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
            crate::workspace_execution::DESKTOP_WRITE_TOOL_REVISION
        );
        assert_eq!(left.installed_agent().runtime_limits.max_iterations, 12);
    }

    #[test]
    fn workspace_installation_requires_and_freezes_the_exact_t1_catalogue() {
        let catalogue = BuiltinT1Catalogue::new("t1.policy.v1", ["source-control"]).unwrap();
        let capabilities = AgentToolCapabilities {
            definitions: catalogue.definitions().to_vec(),
        };
        let installation = builtin_desktop_workspace_agent_installation(
            "garive-workspace",
            "desktop-workspace",
            &capabilities,
        )
        .unwrap();
        assert_eq!(
            installation.installed_agent().definition_revision,
            DESKTOP_WORKSPACE_AGENT_REVISION
        );
        assert_eq!(installation.tool_capabilities().definitions.len(), 6);
        assert!(installation
            .tool_capabilities()
            .definitions
            .iter()
            .any(|definition| definition.name() == "write_file"));

        let mut incomplete = capabilities;
        incomplete.definitions.pop();
        assert!(builtin_desktop_workspace_agent_installation(
            "garive-workspace",
            "desktop-workspace",
            &incomplete,
        )
        .is_err());
    }
}
