use std::{collections::BTreeSet, fs, path::PathBuf};

use garive_config::{
    resolve_definition, AgentDefinition, CapabilityKind, CapabilityReference,
    ContextPolicyCandidate, ContextPolicyReference, DefaultLimits, GovernancePolicy,
    GovernancePolicyCandidate, InstructionReference, InstructionResource, InteractionMode,
    ModelRoleCandidate, ModelRoleRequirement, ProductPolicy, ResolutionErrorCode,
    ResolutionRegistry,
};
use garive_tools::{ExecutionCapability, ExecutionRequirements, ReplayClass, ToolDefinition};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/agent-definition-snapshot.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn modes(value: &Value) -> Vec<InteractionMode> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| match item.as_str().unwrap() {
            "approval" => InteractionMode::Approval,
            "external_input" => InteractionMode::ExternalInput,
            other => panic!("unknown interaction mode: {other}"),
        })
        .collect()
}

fn kind(value: &str) -> CapabilityKind {
    match value {
        "tool" => CapabilityKind::Tool,
        "skill" => CapabilityKind::Skill,
        "memory" => CapabilityKind::Memory,
        "knowledge" => CapabilityKind::Knowledge,
        "delegation" => CapabilityKind::Delegation,
        other => panic!("unknown capability kind: {other}"),
    }
}

fn instruction_reference(value: &Value) -> InstructionReference {
    InstructionReference::new(
        value["source_id"].as_str().unwrap(),
        value["exact_revision"].as_str().unwrap(),
        value["required"].as_bool().unwrap(),
    )
    .unwrap()
}

fn limits(value: &Value) -> DefaultLimits {
    DefaultLimits::new(
        value["max_iterations"].as_u64().unwrap(),
        value["max_input_tokens"].as_u64(),
        value["max_output_tokens"].as_u64(),
        value["deadline_budget_ms"].as_u64(),
    )
    .unwrap()
}

fn definition(fixture: &Value) -> AgentDefinition {
    let value = &fixture["definition"];
    let governance = &value["governance"];
    AgentDefinition::new(
        value["definition_id"].as_str().unwrap(),
        value["revision"].as_str().unwrap(),
        value["instruction_sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(instruction_reference)
            .collect(),
        value["model_roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|role| {
                ModelRoleRequirement::new(
                    role["role_id"].as_str().unwrap(),
                    strings(&role["required_capabilities"]),
                    role["required"].as_bool().unwrap(),
                )
                .unwrap()
            })
            .collect(),
        value["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|capability| {
                CapabilityReference::new(
                    kind(capability["kind"].as_str().unwrap()),
                    capability["name"].as_str().unwrap(),
                    capability["exact_revision"].as_str().unwrap(),
                    capability["contract_version"].as_u64().unwrap(),
                    capability["required"].as_bool().unwrap(),
                )
                .unwrap()
            })
            .collect(),
        GovernancePolicy::new(
            governance["policy_id"].as_str().unwrap(),
            governance["exact_revision"].as_str().unwrap(),
            strings(&governance["allowed_requirement_capabilities"]),
            modes(&governance["interaction_modes"]),
        )
        .unwrap(),
        ContextPolicyReference::new(
            value["context_policy"]["policy_id"].as_str().unwrap(),
            value["context_policy"]["exact_revision"].as_str().unwrap(),
        )
        .unwrap(),
        limits(&value["limits"]),
        value["contract_versions"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, version)| (name.clone(), version.as_u64().unwrap()))
            .collect(),
    )
    .unwrap()
}

fn tool(value: &Value) -> ToolDefinition {
    let requirements = &value["requirements"];
    ToolDefinition::new(
        value["name"].as_str().unwrap(),
        value["revision"].as_str().unwrap(),
        value["description"].as_str().unwrap(),
        value["input_schema"].clone(),
        ExecutionRequirements::new(
            requirements["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| match item.as_str().unwrap() {
                    "filesystem_read" => ExecutionCapability::FilesystemRead,
                    other => panic!("unknown execution capability: {other}"),
                }),
            requirements["max_duration_ms"].as_u64().unwrap(),
            requirements["max_output_bytes"].as_u64().unwrap(),
        )
        .unwrap(),
        ReplayClass::ReadOnly,
    )
    .unwrap()
}

fn registry(fixture: &Value) -> ResolutionRegistry {
    let value = &fixture["registry"];
    ResolutionRegistry {
        instructions: value["instructions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| InstructionResource {
                source_id: item["source_id"].as_str().unwrap().to_owned(),
                exact_revision: item["exact_revision"].as_str().unwrap().to_owned(),
                content_utf8: item["content_utf8"].as_str().unwrap().to_owned(),
                dependencies: item["dependencies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(instruction_reference)
                    .collect(),
            })
            .collect(),
        model_roles: value["model_roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| ModelRoleCandidate {
                role_id: item["role_id"].as_str().unwrap().to_owned(),
                capability_target_id: item["capability_target_id"].as_str().unwrap().to_owned(),
                admitted_capabilities: strings(&item["admitted_capabilities"]),
            })
            .collect(),
        tools: value["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(tool)
            .collect(),
        capability_descriptors: Vec::new(),
        governance_policies: value["governance_policies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| GovernancePolicyCandidate {
                policy_id: item["policy_id"].as_str().unwrap().to_owned(),
                exact_revision: item["exact_revision"].as_str().unwrap().to_owned(),
                allowed_requirement_capabilities: strings(
                    &item["allowed_requirement_capabilities"],
                ),
                interaction_modes: modes(&item["interaction_modes"]).into_iter().collect(),
            })
            .collect(),
        context_policies: value["context_policies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| ContextPolicyCandidate {
                policy_id: item["policy_id"].as_str().unwrap().to_owned(),
                exact_revision: item["exact_revision"].as_str().unwrap().to_owned(),
                descriptor_digest: item["descriptor_digest"].as_str().unwrap().to_owned(),
            })
            .collect(),
    }
}

fn product_policy(fixture: &Value) -> ProductPolicy {
    let value = &fixture["product_policy"];
    ProductPolicy {
        allowed_requirement_capabilities: strings(&value["allowed_requirement_capabilities"]),
        interaction_modes: modes(&value["interaction_modes"]).into_iter().collect(),
        limit_caps: limits(&value["limit_caps"]),
        admitted_contract_versions: value["admitted_contract_versions"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, versions)| {
                (
                    name.clone(),
                    versions
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|version| version.as_u64().unwrap())
                        .collect(),
                )
            })
            .collect(),
    }
}

fn error_code(value: &str) -> ResolutionErrorCode {
    match value {
        "reference_not_found" => ResolutionErrorCode::ReferenceNotFound,
        "reference_ambiguous" => ResolutionErrorCode::ReferenceAmbiguous,
        "reference_cycle" => ResolutionErrorCode::ReferenceCycle,
        "unsupported_contract_version" => ResolutionErrorCode::UnsupportedContractVersion,
        "policy_incompatible" => ResolutionErrorCode::PolicyIncompatible,
        "invalid_definition" => ResolutionErrorCode::InvalidDefinition,
        other => panic!("unknown resolution error: {other}"),
    }
}

#[test]
fn shared_exact_resolution_matches_snapshot() {
    let fixture = fixture();
    let snapshot = resolve_definition(
        &definition(&fixture),
        &registry(&fixture),
        &product_policy(&fixture),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        fixture["expected_snapshot"]
    );
}

#[test]
fn shared_failure_operations_fail_closed() {
    let fixture = fixture();
    for case in fixture["failure_cases"].as_array().unwrap() {
        let mut definition = definition(&fixture);
        let mut registry = registry(&fixture);
        let mut policy = product_policy(&fixture);
        for operation in case["operations"].as_array().unwrap() {
            match operation["kind"].as_str().unwrap() {
                "remove_instruction" => registry.instructions.retain(|item| {
                    item.source_id != operation["source_id"]
                        || item.exact_revision != operation["exact_revision"]
                }),
                "duplicate_instruction" => registry.instructions.push(
                    registry
                        .instructions
                        .iter()
                        .find(|item| {
                            item.source_id == operation["source_id"]
                                && item.exact_revision == operation["exact_revision"]
                        })
                        .unwrap()
                        .clone(),
                ),
                "add_dependency" => registry
                    .instructions
                    .iter_mut()
                    .find(|item| {
                        item.source_id == operation["source_id"]
                            && item.exact_revision == operation["exact_revision"]
                    })
                    .unwrap()
                    .dependencies
                    .push(
                        InstructionReference::new(
                            operation["dependency_source_id"].as_str().unwrap(),
                            operation["dependency_revision"].as_str().unwrap(),
                            true,
                        )
                        .unwrap(),
                    ),
                "set_contract_version" => {
                    definition.contract_versions.insert(
                        operation["contract_name"].as_str().unwrap().to_owned(),
                        operation["version"].as_u64().unwrap(),
                    );
                }
                "remove_product_requirement_capability" => {
                    policy
                        .allowed_requirement_capabilities
                        .remove(operation["capability"].as_str().unwrap());
                }
                "duplicate_instruction_root" => definition.instruction_sources.push(
                    InstructionReference::new(
                        operation["source_id"].as_str().unwrap(),
                        operation["exact_revision"].as_str().unwrap(),
                        true,
                    )
                    .unwrap(),
                ),
                "set_product_max_iterations" => {
                    policy.limit_caps.max_iterations = operation["value"].as_u64().unwrap()
                }
                other => panic!("unknown fixture operation: {other}"),
            }
        }
        let result = if case["name"] == "invalid-duplicate-root" {
            AgentDefinition::new(
                definition.definition_id,
                definition.revision,
                definition.instruction_sources,
                definition.model_roles,
                definition.capabilities,
                definition.governance,
                definition.context_policy,
                definition.limits,
                definition.contract_versions,
            )
            .map(|_| unreachable!())
        } else {
            resolve_definition(&definition, &registry, &policy).map(|_| unreachable!())
        };
        let error = result.unwrap_err();
        assert_eq!(
            error.code(),
            error_code(case["expected_code"].as_str().unwrap()),
            "{}",
            case["name"]
        );
        assert_eq!(
            error.path(),
            case["expected_path"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn continuation_and_limit_properties_hold() {
    let fixture = fixture();
    let definition = definition(&fixture);
    let registry = registry(&fixture);
    let mut policy = product_policy(&fixture);
    let snapshot = resolve_definition(&definition, &registry, &policy).unwrap();
    for case in fixture["continuation_cases"].as_array().unwrap() {
        assert_eq!(
            snapshot
                .validate_continuation(
                    case["definition_revision"].as_str().unwrap(),
                    case["snapshot_digest"].as_str().unwrap()
                )
                .is_ok(),
            case["expected"] == "accepted",
            "{}",
            case["name"]
        );
    }
    for cap in 1..=definition.limits.max_iterations {
        policy.limit_caps.max_iterations = cap;
        let first = resolve_definition(&definition, &registry, &policy).unwrap();
        let second = resolve_definition(&definition, &registry, &policy).unwrap();
        assert_eq!(first, second);
        assert!(first.limits.max_iterations <= definition.limits.max_iterations);
    }
}
