//! Deterministic exact-reference resolution and canonical snapshot binding.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    AgentDefinition, CapabilityKind, EffectiveAgentSnapshot, EffectiveCapabilitySnapshot,
    EffectiveGovernancePolicy, EffectiveLimits, InstructionReference, InstructionResource,
    ProductPolicy, ResolutionError, ResolutionErrorCode, ResolutionRegistry, ResolvedContextPolicy,
    ResolvedInstruction, ResolvedModelRole,
};

fn canonical_digest<T: Serialize>(value: &T, path: &str) -> Result<String, ResolutionError> {
    let bytes = serde_jcs::to_vec(value).map_err(|_| {
        ResolutionError::new(ResolutionErrorCode::NonCanonicalValue, path.to_owned())
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Returns lowercase SHA-256 over RFC 8785 canonical JSON bytes.
pub fn digest_canonical_value(value: &Value) -> Result<String, ResolutionError> {
    canonical_digest(value, "/canonical_value")
}

fn exact_one<'a, T>(
    candidates: Vec<&'a T>,
    required: bool,
    path: &str,
) -> Result<Option<&'a T>, ResolutionError> {
    match candidates.as_slice() {
        [] if required => Err(ResolutionError::new(
            ResolutionErrorCode::ReferenceNotFound,
            path,
        )),
        [] => Ok(None),
        [candidate] => Ok(Some(*candidate)),
        _ => Err(ResolutionError::new(
            ResolutionErrorCode::ReferenceAmbiguous,
            path,
        )),
    }
}

struct InstructionExpansion<'a> {
    registry: &'a ResolutionRegistry,
    active: BTreeSet<(String, String)>,
    emitted: BTreeSet<(String, String)>,
    output: Vec<ResolvedInstruction>,
}

impl<'a> InstructionExpansion<'a> {
    fn expand(
        &mut self,
        reference: &InstructionReference,
        path: &str,
        cycle_path: &str,
    ) -> Result<(), ResolutionError> {
        let key = (
            reference.source_id.clone(),
            reference.exact_revision.clone(),
        );
        if self.emitted.contains(&key) {
            return Ok(());
        }
        if !self.active.insert(key.clone()) {
            return Err(ResolutionError::new(
                ResolutionErrorCode::ReferenceCycle,
                cycle_path,
            ));
        }
        let candidates = self
            .registry
            .instructions
            .iter()
            .filter(|item| {
                item.source_id == reference.source_id
                    && item.exact_revision == reference.exact_revision
            })
            .collect();
        let Some(resource) = exact_one(candidates, reference.required, path)? else {
            self.active.remove(&key);
            return Ok(());
        };
        self.expand_dependencies(resource, path, cycle_path)?;
        self.active.remove(&key);
        self.emitted.insert(key);
        self.output.push(ResolvedInstruction {
            source_id: resource.source_id.clone(),
            exact_revision: resource.exact_revision.clone(),
            content_utf8: resource.content_utf8.clone(),
            content_digest: format!("{:x}", Sha256::digest(resource.content_utf8.as_bytes())),
        });
        Ok(())
    }

    fn expand_dependencies(
        &mut self,
        resource: &InstructionResource,
        path: &str,
        cycle_path: &str,
    ) -> Result<(), ResolutionError> {
        for (index, dependency) in resource.dependencies.iter().enumerate() {
            self.expand(
                dependency,
                &format!("{path}/dependencies/{index}"),
                cycle_path,
            )?;
        }
        Ok(())
    }
}

fn resolve_instructions(
    definition: &AgentDefinition,
    registry: &ResolutionRegistry,
) -> Result<Vec<ResolvedInstruction>, ResolutionError> {
    let mut expansion = InstructionExpansion {
        registry,
        active: BTreeSet::new(),
        emitted: BTreeSet::new(),
        output: Vec::new(),
    };
    for (index, reference) in definition.instruction_sources.iter().enumerate() {
        let path = format!("/instruction_sources/{index}");
        expansion.expand(reference, &path, &path)?;
    }
    Ok(expansion.output)
}

fn resolve_roles(
    definition: &AgentDefinition,
    registry: &ResolutionRegistry,
) -> Result<Vec<ResolvedModelRole>, ResolutionError> {
    let mut output = Vec::new();
    for (index, requirement) in definition.model_roles.iter().enumerate() {
        let path = format!("/model_roles/{index}");
        let candidates = registry
            .model_roles
            .iter()
            .filter(|candidate| candidate.role_id == requirement.role_id)
            .collect();
        let Some(candidate) = exact_one(candidates, requirement.required, &path)? else {
            continue;
        };
        if !requirement
            .required_capabilities
            .is_subset(&candidate.admitted_capabilities)
        {
            if requirement.required {
                return Err(ResolutionError::new(
                    ResolutionErrorCode::PolicyIncompatible,
                    path,
                ));
            }
            continue;
        }
        output.push(ResolvedModelRole {
            role_id: requirement.role_id.clone(),
            capability_target_id: candidate.capability_target_id.clone(),
            admitted_capabilities: requirement.required_capabilities.clone(),
        });
    }
    Ok(output)
}

fn tighten_limit(
    requested: Option<u64>,
    cap: Option<u64>,
    path: &str,
) -> Result<Option<u64>, ResolutionError> {
    match (requested, cap) {
        (Some(requested), Some(cap)) if cap > requested => Err(ResolutionError::new(
            ResolutionErrorCode::PolicyIncompatible,
            path,
        )),
        (Some(_), Some(cap)) | (None, Some(cap)) => Ok(Some(cap)),
        (Some(requested), None) => Ok(Some(requested)),
        (None, None) => Ok(None),
    }
}

fn effective_limits(
    definition: &AgentDefinition,
    policy: &ProductPolicy,
) -> Result<EffectiveLimits, ResolutionError> {
    if policy.limit_caps.max_iterations > definition.limits.max_iterations {
        return Err(ResolutionError::new(
            ResolutionErrorCode::PolicyIncompatible,
            "/limits/max_iterations",
        ));
    }
    Ok(EffectiveLimits {
        max_iterations: policy.limit_caps.max_iterations,
        max_input_tokens: tighten_limit(
            definition.limits.max_input_tokens,
            policy.limit_caps.max_input_tokens,
            "/limits/max_input_tokens",
        )?,
        max_output_tokens: tighten_limit(
            definition.limits.max_output_tokens,
            policy.limit_caps.max_output_tokens,
            "/limits/max_output_tokens",
        )?,
        deadline_budget_ms: tighten_limit(
            definition.limits.deadline_budget_ms,
            policy.limit_caps.deadline_budget_ms,
            "/limits/deadline_budget_ms",
        )?,
    })
}

fn validate_contracts(
    definition: &AgentDefinition,
    policy: &ProductPolicy,
) -> Result<(), ResolutionError> {
    for (name, version) in &definition.contract_versions {
        if !policy
            .admitted_contract_versions
            .get(name)
            .is_some_and(|versions| versions.contains(version))
        {
            return Err(ResolutionError::new(
                ResolutionErrorCode::UnsupportedContractVersion,
                format!("/contract_versions/{name}"),
            ));
        }
    }
    Ok(())
}

/// Resolves exact Runtime candidates into one immutable Turn-bound snapshot.
pub fn resolve_definition(
    definition: &AgentDefinition,
    registry: &ResolutionRegistry,
    policy: &ProductPolicy,
) -> Result<EffectiveAgentSnapshot, ResolutionError> {
    validate_contracts(definition, policy)?;
    let policy_path = "/governance";
    let governance_candidates = registry
        .governance_policies
        .iter()
        .filter(|candidate| {
            candidate.policy_id == definition.governance.policy_id
                && candidate.exact_revision == definition.governance.exact_revision
        })
        .collect();
    let governance = exact_one(governance_candidates, true, policy_path)?.expect("required");
    let allowed_requirement_capabilities = definition
        .governance
        .allowed_requirement_capabilities
        .intersection(&governance.allowed_requirement_capabilities)
        .filter(|value| policy.allowed_requirement_capabilities.contains(*value))
        .cloned()
        .collect();
    let interaction_modes = definition
        .governance
        .interaction_modes
        .intersection(&governance.interaction_modes)
        .filter(|value| policy.interaction_modes.contains(value))
        .copied()
        .collect();
    let effective_governance = EffectiveGovernancePolicy {
        policy_id: governance.policy_id.clone(),
        exact_revision: governance.exact_revision.clone(),
        allowed_requirement_capabilities,
        interaction_modes,
        default_unmatched: definition.governance.default_unmatched,
    };
    let mut tools = Vec::new();
    let mut descriptors = Vec::new();
    for reference in &definition.capabilities {
        let path = format!("/capabilities/{:?}/{}", reference.kind, reference.name).to_lowercase();
        if reference.kind == CapabilityKind::Tool {
            let candidates = registry
                .tools
                .iter()
                .filter(|tool| {
                    tool.name() == reference.name && tool.revision() == reference.exact_revision
                })
                .collect();
            let Some(tool) = exact_one(candidates, reference.required, &path)? else {
                continue;
            };
            let requirements_admitted = tool.requirements().capabilities().all(|capability| {
                effective_governance
                    .allowed_requirement_capabilities
                    .contains(capability.wire_name())
            });
            if !requirements_admitted {
                if reference.required {
                    return Err(ResolutionError::new(
                        ResolutionErrorCode::PolicyIncompatible,
                        path,
                    ));
                }
                continue;
            }
            tools.push(tool.clone());
        } else {
            let candidates = registry
                .capability_descriptors
                .iter()
                .filter(|candidate| {
                    candidate.kind == reference.kind
                        && candidate.name == reference.name
                        && candidate.exact_revision == reference.exact_revision
                        && candidate.contract_version == reference.contract_version
                })
                .collect();
            if let Some(candidate) = exact_one(candidates, reference.required, &path)? {
                descriptors.push(candidate.clone());
            }
        }
    }
    let context_candidates = registry
        .context_policies
        .iter()
        .filter(|candidate| {
            candidate.policy_id == definition.context_policy.policy_id
                && candidate.exact_revision == definition.context_policy.exact_revision
        })
        .collect();
    let context = exact_one(context_candidates, true, "/context_policy")?.expect("required");
    let definition_preimage = json!({
        "contract": "garive.agent-definition",
        "version": 1,
        "definition": definition,
    });
    let mut snapshot = EffectiveAgentSnapshot {
        definition_id: definition.definition_id.clone(),
        definition_revision: definition.revision.clone(),
        definition_digest: digest_canonical_value(&definition_preimage)?,
        instructions: resolve_instructions(definition, registry)?,
        model_roles: resolve_roles(definition, registry)?,
        capabilities: EffectiveCapabilitySnapshot { tools, descriptors },
        governance: effective_governance,
        context_policy: ResolvedContextPolicy {
            policy_id: context.policy_id.clone(),
            exact_revision: context.exact_revision.clone(),
            descriptor_digest: context.descriptor_digest.clone(),
        },
        limits: effective_limits(definition, policy)?,
        contract_versions: definition.contract_versions.clone(),
        snapshot_digest: String::new(),
    };
    let mut preimage = serde_json::to_value(&snapshot)
        .map_err(|_| ResolutionError::new(ResolutionErrorCode::NonCanonicalValue, "/snapshot"))?;
    let object = preimage
        .as_object_mut()
        .expect("snapshot serializes as object");
    object.remove("snapshot_digest");
    object.insert(
        "contract".to_owned(),
        Value::String("garive.effective-agent-snapshot".to_owned()),
    );
    object.insert("version".to_owned(), Value::from(1));
    snapshot.snapshot_digest = digest_canonical_value(&preimage)?;
    Ok(snapshot)
}
