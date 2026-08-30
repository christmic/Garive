use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionCapability, ExecutionRequirements,
    InvocationAccessSet, PreparationError, PreparationErrorCode, ReplayClass, ResourceAccess,
    ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition, ToolIntent,
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

fn definition() -> ToolDefinition {
    ToolDefinition::new_v2(
        "read_file",
        "read-file-v2",
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
    )
    .unwrap()
}

#[test]
fn prepared_v2_binds_exact_accesses_and_result_charge() {
    let catalog = ToolCatalog::new([definition()]).unwrap();
    let prepared = catalog
        .prepare_v2(
            &ToolIntent::new("call-1", "read_file", r#"{"path":"src/lib.rs"}"#),
            &PathResolver,
        )
        .unwrap();

    assert_eq!(prepared.contract_version(), 2);
    assert_eq!(prepared.access_policy_revision(), Some("read-policy-v1"));
    assert_eq!(
        prepared.access_resolver_revision(),
        Some("path-resolver-v1")
    );
    assert_eq!(prepared.max_result_bytes(), Some(2_048));
    assert_eq!(
        prepared.invocation_accesses().unwrap().values()[0].resource_key(),
        "src/lib.rs"
    );
    assert_eq!(prepared.input_digest().len(), 64);
}

#[test]
fn v2_rejects_missing_wrong_or_out_of_policy_resolver() {
    struct WrongResolver;
    impl ToolAccessResolver for WrongResolver {
        fn revision(&self) -> &str {
            "wrong"
        }
        fn resolve(&self, _: &Value) -> Result<InvocationAccessSet, PreparationError> {
            unreachable!()
        }
    }

    let catalog = ToolCatalog::new([definition()]).unwrap();
    let intent = ToolIntent::new("call-1", "read_file", r#"{"path":"outside/file"}"#);
    assert_eq!(
        catalog.prepare(&intent).unwrap_err().code(),
        PreparationErrorCode::EffectAccessInvalid
    );
    assert_eq!(
        catalog
            .prepare_v2(&intent, &WrongResolver)
            .unwrap_err()
            .code(),
        PreparationErrorCode::EffectAccessInvalid
    );
    assert_eq!(
        catalog
            .prepare_v2(&intent, &PathResolver)
            .unwrap_err()
            .code(),
        PreparationErrorCode::EffectAccessInvalid
    );
}
