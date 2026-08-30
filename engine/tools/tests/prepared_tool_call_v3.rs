use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionCapability, ExecutionRequirements,
    InvocationAccessSet, PreparationError, PreparationErrorCode, ReplayClass, ResourceAccess,
    SandboxControl, SandboxRequirementsV1, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog,
    ToolDefinition, ToolIntent,
};
use serde_json::{json, Value};

struct PathResolver;

impl ToolAccessResolver for PathResolver {
    fn revision(&self) -> &str {
        "path-resolver-v1"
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap(),
            AccessMode::Read,
        )?])
    }
}

#[test]
fn prepared_v3_binds_exact_sandbox_profile_and_digest() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/sandbox-safety-v1.json"
    ))
    .unwrap();
    let expected = &fixture["prepared_v3"];
    let catalog = ToolCatalog::new([definition(filesystem_profile())]).unwrap();
    let prepared = catalog
        .prepare_v3(
            &ToolIntent::new("call-1", "read_file", r#"{"path":"src/lib.rs"}"#),
            &PathResolver,
        )
        .unwrap();

    assert_eq!(prepared.contract_version(), 3);
    assert_eq!(
        prepared.sandbox_requirements_digest(),
        expected["sandbox_requirements_digest"].as_str()
    );
    assert_eq!(prepared.sandbox_requirements().unwrap().max_open_files(), 8);
    assert_eq!(
        prepared.input_digest(),
        expected["prepared_digest"].as_str().unwrap()
    );
}

#[test]
fn v3_definition_revalidates_profile_against_exact_capabilities() {
    let process_profile = SandboxRequirementsV1::new(
        [ExecutionCapability::Process],
        [
            SandboxControl::ProcessContainment,
            SandboxControl::StructuredArguments,
            SandboxControl::EnvironmentAllowlist,
            SandboxControl::ResourceLimits,
        ],
        Some(1),
        8,
    )
    .unwrap();
    assert_eq!(
        definition_result(process_profile).unwrap_err().code(),
        PreparationErrorCode::SandboxRequirementInvalid
    );
}

#[test]
fn v3_definition_cannot_be_prepared_through_v2() {
    let catalog = ToolCatalog::new([definition(filesystem_profile())]).unwrap();
    let error = catalog
        .prepare_v2(
            &ToolIntent::new("call-1", "read_file", r#"{"path":"src/lib.rs"}"#),
            &PathResolver,
        )
        .unwrap_err();
    assert_eq!(
        error.code(),
        PreparationErrorCode::SandboxRequirementInvalid
    );
}

fn filesystem_profile() -> SandboxRequirementsV1 {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemRead],
        [
            SandboxControl::FilesystemScope,
            SandboxControl::SymlinkContainment,
            SandboxControl::ResourceLimits,
        ],
        None,
        8,
    )
    .unwrap()
}

fn definition(sandbox: SandboxRequirementsV1) -> ToolDefinition {
    definition_result(sandbox).unwrap()
}

fn definition_result(sandbox: SandboxRequirementsV1) -> Result<ToolDefinition, PreparationError> {
    ToolDefinition::new_v3(
        "read_file",
        "read-file-v3",
        "Read one file",
        json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": false
        }),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 4_096).unwrap(),
        ReplayClass::ReadOnly,
        ToolAccessPolicyV1::new(
            "read-policy-v1",
            [AccessPolicyEntry::new("src", [AccessMode::Read]).unwrap()],
            [],
            [],
            [],
            1,
            2_048,
        )
        .unwrap(),
        "path-resolver-v1",
        sandbox,
    )
}
