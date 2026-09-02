//! Built-in Desktop Agent definition and its exact Runtime installation.

use std::collections::{BTreeMap, BTreeSet};

use garive_config::{
    resolve_definition, AgentDefinition, CapabilityDescriptor, CapabilityKind, CapabilityReference,
    ContextPolicyCandidate, ContextPolicyReference, DefaultLimits, GovernancePolicy,
    GovernancePolicyCandidate, InteractionMode, ProductPolicy, ResolutionRegistry,
};
use garive_core::AgentToolCapabilities;
use garive_runtime::RuntimeAgentInstallation;
use garive_tools::{
    ToolDefinition, T1_APPLY_PATCH, T1_LIST, T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT,
    T1_TOOL_REVISION, T1_WRITE_TEXT,
};

use crate::workspace_execution::desktop_workspace_tool_definition;

/// Exact revision of the built-in Desktop Agent definition.
pub const DESKTOP_AGENT_REVISION: &str = "desktop.agent.v3";
/// Exact revision of the built-in T1 Workspace Agent definition.
pub const DESKTOP_WORKSPACE_AGENT_REVISION: &str = "desktop.workspace-agent.v3";
pub(crate) const MEMORY_DESKTOP_AGENT_REVISION: &str = "desktop.agent.v2";
pub(crate) const MEMORY_DESKTOP_WORKSPACE_AGENT_REVISION: &str = "desktop.workspace-agent.v2";
pub(crate) const LEGACY_DESKTOP_AGENT_REVISION: &str = "desktop.agent.v1";
pub(crate) const LEGACY_DESKTOP_WORKSPACE_AGENT_REVISION: &str = "desktop.workspace-agent.v1";
/// Stable local Memory capability installed by Desktop Agent v2.
pub const DESKTOP_MEMORY_CAPABILITY_NAME: &str = "memory.local";
/// Exact local Memory descriptor revision installed by Desktop Agent v2.
pub const DESKTOP_MEMORY_CAPABILITY_REVISION: &str = "memory.local.v1";
/// Digest of the v1 Desktop local Memory descriptor meaning.
pub const DESKTOP_MEMORY_DESCRIPTOR_DIGEST: &str =
    "d8fb67fd95277dc8268778a19b13c0dbf8c8cfde7686b2853245c4ca75f4b02e";
/// Stable static Knowledge capability installed by Desktop Agent v3.
pub const DESKTOP_KNOWLEDGE_CAPABILITY_NAME: &str = "knowledge.static";
/// Exact static Knowledge descriptor revision installed by Desktop Agent v3.
pub const DESKTOP_KNOWLEDGE_CAPABILITY_REVISION: &str = "knowledge.static.v1";
/// Digest of the v1 Desktop static Knowledge descriptor meaning.
pub const DESKTOP_KNOWLEDGE_DESCRIPTOR_DIGEST: &str =
    "c70e7bfbd86f858665c271f6fc8ae32bc47cba03b2deee1d11150cb969a7c8ff";
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
    desktop_agent_installation_for_revision(
        definition_id,
        DESKTOP_AGENT_REVISION,
        instance_namespace,
        true,
        true,
    )
}

pub(crate) fn desktop_agent_installation_for_revision(
    definition_id: &str,
    definition_revision: &str,
    instance_namespace: &str,
    memory: bool,
    knowledge: bool,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    let tool = desktop_workspace_tool_definition().map_err(|_| DesktopAgentCompositionError)?;
    resolve_desktop_installation(
        definition_id,
        definition_revision,
        instance_namespace,
        vec![tool],
        GOVERNANCE_ID,
        GOVERNANCE_REVISION,
        BTreeSet::from(["filesystem_write".to_owned()]),
        memory,
        knowledge,
    )
}

/// Resolves the exact T1 Workspace Agent from machine capabilities.
pub fn builtin_desktop_workspace_agent_installation(
    definition_id: &str,
    instance_namespace: &str,
    t1: &AgentToolCapabilities,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    desktop_workspace_agent_installation_for_revision(
        definition_id,
        DESKTOP_WORKSPACE_AGENT_REVISION,
        instance_namespace,
        t1,
        true,
        true,
    )
}

pub(crate) fn desktop_workspace_agent_installation_for_revision(
    definition_id: &str,
    definition_revision: &str,
    instance_namespace: &str,
    t1: &AgentToolCapabilities,
    memory: bool,
    knowledge: bool,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    let expected = BTreeSet::from([
        T1_APPLY_PATCH,
        T1_LIST,
        T1_PROCESS_RUN,
        T1_READ_TEXT,
        T1_SEARCH_TEXT,
        T1_WRITE_TEXT,
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
        definition_revision,
        instance_namespace,
        tools,
        WORKSPACE_GOVERNANCE_ID,
        WORKSPACE_GOVERNANCE_REVISION,
        BTreeSet::from([
            "filesystem_read".to_owned(),
            "filesystem_write".to_owned(),
            "process".to_owned(),
        ]),
        memory,
        knowledge,
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
    memory: bool,
    knowledge: bool,
) -> Result<RuntimeAgentInstallation, DesktopAgentCompositionError> {
    let interaction_modes = BTreeSet::from([InteractionMode::Approval]);
    let limits = DefaultLimits::new(12, Some(131_072), Some(8_192), Some(600_000))
        .map_err(|_| DesktopAgentCompositionError)?;
    let memory_descriptor = CapabilityDescriptor {
        kind: CapabilityKind::Memory,
        name: DESKTOP_MEMORY_CAPABILITY_NAME.into(),
        exact_revision: DESKTOP_MEMORY_CAPABILITY_REVISION.into(),
        contract_version: garive_runtime::LOCAL_MEMORY_CONTRACT_VERSION,
        descriptor_digest: DESKTOP_MEMORY_DESCRIPTOR_DIGEST.into(),
    };
    let knowledge_descriptor = CapabilityDescriptor {
        kind: CapabilityKind::Knowledge,
        name: DESKTOP_KNOWLEDGE_CAPABILITY_NAME.into(),
        exact_revision: DESKTOP_KNOWLEDGE_CAPABILITY_REVISION.into(),
        contract_version: garive_runtime::LOCAL_KNOWLEDGE_CONTRACT_VERSION,
        descriptor_digest: DESKTOP_KNOWLEDGE_DESCRIPTOR_DIGEST.into(),
    };
    let mut capabilities = tools
        .iter()
        .map(|tool| {
            CapabilityReference::new(CapabilityKind::Tool, tool.name(), tool.revision(), 3, true)
                .map_err(|_| DesktopAgentCompositionError)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if memory {
        capabilities.push(
            CapabilityReference::new(
                CapabilityKind::Memory,
                DESKTOP_MEMORY_CAPABILITY_NAME,
                DESKTOP_MEMORY_CAPABILITY_REVISION,
                garive_runtime::LOCAL_MEMORY_CONTRACT_VERSION,
                true,
            )
            .map_err(|_| DesktopAgentCompositionError)?,
        );
    }
    if knowledge {
        capabilities.push(
            CapabilityReference::new(
                CapabilityKind::Knowledge,
                DESKTOP_KNOWLEDGE_CAPABILITY_NAME,
                DESKTOP_KNOWLEDGE_CAPABILITY_REVISION,
                garive_runtime::LOCAL_KNOWLEDGE_CONTRACT_VERSION,
                true,
            )
            .map_err(|_| DesktopAgentCompositionError)?,
        );
    }
    let definition = AgentDefinition::new(
        definition_id,
        definition_revision,
        Vec::new(),
        Vec::new(),
        capabilities,
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
            capability_descriptors: memory
                .then_some(memory_descriptor)
                .into_iter()
                .chain(knowledge.then_some(knowledge_descriptor))
                .collect(),
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
        assert_eq!(left.snapshot().capabilities().descriptors.len(), 2);
        assert_eq!(
            left.snapshot()
                .capabilities()
                .descriptors
                .iter()
                .map(|descriptor| descriptor.name.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                DESKTOP_KNOWLEDGE_CAPABILITY_NAME,
                DESKTOP_MEMORY_CAPABILITY_NAME,
            ])
        );
        assert_eq!(
            left.tool_capabilities().definitions[0].revision(),
            crate::workspace_execution::DESKTOP_WRITE_TOOL_REVISION
        );
        assert_eq!(left.installed_agent().runtime_limits.max_iterations, 12);
        let legacy = desktop_agent_installation_for_revision(
            "garive-work",
            LEGACY_DESKTOP_AGENT_REVISION,
            "desktop-main",
            false,
            false,
        )
        .unwrap();
        assert!(legacy.snapshot().capabilities().descriptors.is_empty());
        let memory = desktop_agent_installation_for_revision(
            "garive-work",
            MEMORY_DESKTOP_AGENT_REVISION,
            "desktop-main",
            true,
            false,
        )
        .unwrap();
        assert_eq!(memory.snapshot().capabilities().descriptors.len(), 1);
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
        assert_eq!(installation.tool_capabilities().definitions.len(), 7);
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
