//! Runtime installation of one resolved, immutable Agent snapshot.

use std::collections::BTreeSet;

use garive_config::EffectiveAgentSnapshot;
use garive_core::AgentToolCapabilities;

use crate::{
    EffectiveRuntimeLimits, InstalledActivityCatalogue, InstalledActivityDescriptor, InstalledAgent,
};

/// Stable failure while projecting a resolved D0 snapshot into Runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeAgentInstallationError {
    /// The host namespace or public projection is malformed.
    InvalidProjection,
    /// Governed execution capabilities differ from the installed snapshot.
    CapabilityMismatch,
    /// A durable continuation does not reuse the exact snapshot binding.
    SnapshotMismatch,
}

/// One D0 snapshot plus the exact Runtime and Core projections derived from it.
#[derive(Clone, Debug)]
pub struct RuntimeAgentInstallation {
    snapshot: EffectiveAgentSnapshot,
    installed: InstalledAgent,
    tools: AgentToolCapabilities,
}

impl RuntimeAgentInstallation {
    /// Derives every executable Agent projection from one resolved snapshot.
    pub fn new(
        snapshot: EffectiveAgentSnapshot,
        agent_instance_namespace: impl Into<String>,
        public_capabilities: Vec<String>,
    ) -> Result<Self, RuntimeAgentInstallationError> {
        let agent_instance_namespace = agent_instance_namespace.into();
        if agent_instance_namespace.is_empty()
            || public_capabilities.iter().any(String::is_empty)
            || !strictly_sorted_unique(&public_capabilities)
        {
            return Err(RuntimeAgentInstallationError::InvalidProjection);
        }
        let limits = snapshot.limits();
        let installed = InstalledAgent {
            definition_id: snapshot.definition_id().to_owned(),
            definition_revision: snapshot.definition_revision().to_owned(),
            snapshot_digest: snapshot.snapshot_digest().to_owned(),
            agent_instance_namespace,
            public_capabilities,
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: limits.max_iterations,
                max_input_tokens: limits.max_input_tokens,
                max_output_tokens: limits.max_output_tokens,
                deadline_budget_ms: limits.deadline_budget_ms,
            },
            public_activity_catalogue: snapshot.public_tool_activity_catalogue().map(|catalogue| {
                InstalledActivityCatalogue {
                    schema_version: catalogue.schema_version,
                    catalogue_revision: catalogue.catalogue_revision.clone(),
                    descriptors: catalogue
                        .descriptors
                        .iter()
                        .map(|descriptor| InstalledActivityDescriptor {
                            tool_name: descriptor.tool_name.clone(),
                            tool_revision: descriptor.tool_revision.clone(),
                            label_key: descriptor.label_key.clone(),
                        })
                        .collect(),
                }
            }),
        };
        let tools = AgentToolCapabilities {
            definitions: snapshot.capabilities().tools.clone(),
        };
        Ok(Self {
            snapshot,
            installed,
            tools,
        })
    }

    /// Returns the immutable effective snapshot owned by this installation.
    pub const fn snapshot(&self) -> &EffectiveAgentSnapshot {
        &self.snapshot
    }

    /// Returns the Host projection derived from the effective snapshot.
    pub const fn installed_agent(&self) -> &InstalledAgent {
        &self.installed
    }

    /// Returns the exact C4 definitions supplied to one Core execution.
    pub const fn tool_capabilities(&self) -> &AgentToolCapabilities {
        &self.tools
    }

    /// Produces an owned Host projection for composition roots.
    pub fn clone_installed_agent(&self) -> InstalledAgent {
        self.installed.clone()
    }

    /// Rejects a factory that invents, removes, reorders or revises tools.
    pub fn validate_tool_capabilities(
        &self,
        capabilities: &AgentToolCapabilities,
    ) -> Result<(), RuntimeAgentInstallationError> {
        if capabilities == &self.tools {
            Ok(())
        } else {
            Err(RuntimeAgentInstallationError::CapabilityMismatch)
        }
    }

    /// Rejects continuation under a changed definition revision or snapshot.
    pub fn validate_continuation(
        &self,
        definition_revision: &str,
        snapshot_digest: &str,
    ) -> Result<(), RuntimeAgentInstallationError> {
        self.snapshot
            .validate_continuation(definition_revision, snapshot_digest)
            .map_err(|_| RuntimeAgentInstallationError::SnapshotMismatch)
    }
}

fn strictly_sorted_unique(values: &[String]) -> bool {
    let set: BTreeSet<_> = values.iter().collect();
    set.len() == values.len() && values.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use garive_config::{
        resolve_definition, AgentDefinition, CapabilityKind, CapabilityReference,
        ContextPolicyCandidate, ContextPolicyReference, DefaultLimits, GovernancePolicy,
        GovernancePolicyCandidate, ProductPolicy, ResolutionRegistry,
    };
    use garive_tools::{ExecutionCapability, ExecutionRequirements, ReplayClass, ToolDefinition};
    use serde_json::json;

    use super::*;

    #[test]
    fn installation_derives_host_and_core_from_one_snapshot() {
        let snapshot = snapshot();
        let installation = RuntimeAgentInstallation::new(
            snapshot.clone(),
            "desktop-installation",
            vec!["activity".into(), "workspaces".into()],
        )
        .unwrap();
        assert_eq!(
            installation.installed_agent().snapshot_digest,
            snapshot.snapshot_digest()
        );
        assert_eq!(installation.tool_capabilities().definitions.len(), 1);
        installation
            .validate_tool_capabilities(installation.tool_capabilities())
            .unwrap();
        installation
            .validate_continuation(snapshot.definition_revision(), snapshot.snapshot_digest())
            .unwrap();

        let mut changed = installation.tool_capabilities().clone();
        changed.definitions.clear();
        assert_eq!(
            installation.validate_tool_capabilities(&changed),
            Err(RuntimeAgentInstallationError::CapabilityMismatch)
        );
        assert_eq!(
            installation.validate_continuation("changed", snapshot.snapshot_digest()),
            Err(RuntimeAgentInstallationError::SnapshotMismatch)
        );
    }

    #[test]
    fn public_projection_must_be_canonical() {
        for values in [
            vec!["workspaces".into(), "activity".into()],
            vec!["activity".into(), "activity".into()],
            vec![String::new()],
        ] {
            assert!(RuntimeAgentInstallation::new(snapshot(), "namespace", values).is_err());
        }
    }

    fn snapshot() -> EffectiveAgentSnapshot {
        let requirements =
            ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 4_096)
                .unwrap();
        let tool = ToolDefinition::new(
            "workspace.read",
            "workspace.read.v1",
            "Read one file.",
            json!({
                "$schema":"https://json-schema.org/draft/2020-12/schema",
                "type":"object",
                "properties":{"path":{"type":"string","minLength":1}},
                "required":["path"],
                "additionalProperties":false
            }),
            requirements,
            ReplayClass::ReadOnly,
        )
        .unwrap();
        let capabilities = BTreeSet::from(["filesystem_read".to_owned()]);
        let limits = DefaultLimits::new(8, Some(16_384), Some(4_096), Some(30_000)).unwrap();
        let definition = AgentDefinition::new(
            "agent",
            "agent.v1",
            Vec::new(),
            Vec::new(),
            vec![CapabilityReference::new(
                CapabilityKind::Tool,
                tool.name(),
                tool.revision(),
                1,
                true,
            )
            .unwrap()],
            GovernancePolicy::new("governance", "governance.v1", capabilities.clone(), []).unwrap(),
            ContextPolicyReference::new("context", "context.v1").unwrap(),
            limits.clone(),
            BTreeMap::from([("effective_snapshot".into(), 1)]),
        )
        .unwrap();
        resolve_definition(
            &definition,
            &ResolutionRegistry {
                instructions: Vec::new(),
                model_roles: Vec::new(),
                tools: vec![tool],
                capability_descriptors: Vec::new(),
                governance_policies: vec![GovernancePolicyCandidate {
                    policy_id: "governance".into(),
                    exact_revision: "governance.v1".into(),
                    allowed_requirement_capabilities: capabilities.clone(),
                    interaction_modes: BTreeSet::new(),
                }],
                context_policies: vec![ContextPolicyCandidate {
                    policy_id: "context".into(),
                    exact_revision: "context.v1".into(),
                    descriptor_digest: "a".repeat(64),
                }],
                public_tool_activity_catalogue: None,
            },
            &ProductPolicy {
                allowed_requirement_capabilities: capabilities,
                interaction_modes: BTreeSet::new(),
                limit_caps: limits,
                admitted_contract_versions: BTreeMap::from([(
                    "effective_snapshot".into(),
                    BTreeSet::from([1]),
                )]),
            },
        )
        .unwrap()
    }
}
