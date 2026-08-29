use std::{fs, path::PathBuf};

use garive_tools::{
    ExecutionCapability, ExecutionRequirements, PreparationErrorCode, ReplayClass, ToolCatalog,
    ToolDefinition, ToolIntent,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/prepared-tool-call.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn capability(value: &str) -> ExecutionCapability {
    match value {
        "filesystem_read" => ExecutionCapability::FilesystemRead,
        "filesystem_write" => ExecutionCapability::FilesystemWrite,
        "process" => ExecutionCapability::Process,
        "network" => ExecutionCapability::Network,
        other => panic!("unknown fixture capability: {other}"),
    }
}

fn replay_class(value: &str) -> ReplayClass {
    match value {
        "read_only" => ReplayClass::ReadOnly,
        "idempotent" => ReplayClass::Idempotent,
        "receipt_recoverable" => ReplayClass::ReceiptRecoverable,
        "never_replay" => ReplayClass::NeverReplay,
        other => panic!("unknown fixture replay class: {other}"),
    }
}

fn definition(value: &Value) -> Result<ToolDefinition, garive_tools::PreparationError> {
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
                .map(|item| capability(item.as_str().unwrap())),
            requirements["max_duration_ms"].as_u64().unwrap(),
            requirements["max_output_bytes"].as_u64().unwrap(),
        )?,
        replay_class(value["replay_class"].as_str().unwrap()),
    )
}

fn catalog(fixture: &Value) -> ToolCatalog {
    ToolCatalog::new(
        fixture["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| definition(value).unwrap()),
    )
    .unwrap()
}

fn code(value: &str) -> PreparationErrorCode {
    match value {
        "invalid_model_call_id" => PreparationErrorCode::InvalidModelCallId,
        "invalid_tool_name" => PreparationErrorCode::InvalidToolName,
        "tool_not_admitted" => PreparationErrorCode::ToolNotAdmitted,
        "invalid_arguments_json" => PreparationErrorCode::InvalidArgumentsJson,
        "arguments_schema_mismatch" => PreparationErrorCode::ArgumentsSchemaMismatch,
        "invalid_tool_definition" => PreparationErrorCode::InvalidToolDefinition,
        "unsupported_schema_keyword" => PreparationErrorCode::UnsupportedSchemaKeyword,
        "non_canonical_value" => PreparationErrorCode::NonCanonicalValue,
        other => panic!("unknown fixture error: {other}"),
    }
}

#[test]
fn shared_preparation_cases_match() {
    let fixture = fixture();
    let catalog = catalog(&fixture);

    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let result = catalog.prepare(&ToolIntent::new(
            input["model_call_id"].as_str().unwrap(),
            input["tool_name"].as_str().unwrap(),
            input["arguments_json"].as_str().unwrap(),
        ));
        let expected = &case["expected"];
        if expected["status"] == "prepared" {
            let prepared = result
                .unwrap_or_else(|error| panic!("{} unexpectedly failed: {error:?}", case["name"]));
            assert_eq!(
                prepared.normalized_arguments(),
                expected["normalized_arguments"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(
                prepared.input_digest(),
                expected["input_digest"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let error = result.expect_err(case["name"].as_str().unwrap());
            assert_eq!(error.code(), code(expected["code"].as_str().unwrap()));
            if let Some(instance_path) = expected["instance_path"].as_str() {
                let failure = error.failures().first().unwrap();
                assert_eq!(failure.instance_path(), instance_path);
                assert_eq!(
                    failure.schema_path(),
                    expected["schema_path"].as_str().unwrap()
                );
                assert_eq!(failure.keyword(), expected["keyword"].as_str().unwrap());
            }
        }
    }
}

#[test]
fn invalid_definition_cases_fail_before_catalog_use() {
    let fixture = fixture();
    for case in fixture["invalid_definitions"].as_array().unwrap() {
        let result = ToolDefinition::new(
            case["name"].as_str().unwrap(),
            "1",
            "invalid fixture definition",
            case["schema"].clone(),
            ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1, 1).unwrap(),
            ReplayClass::ReadOnly,
        );
        assert_eq!(
            result.unwrap_err().code(),
            code(case["expected_code"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn duplicate_catalog_names_and_invalid_requirements_fail() {
    let fixture = fixture();
    let first = definition(&fixture["definitions"][0]).unwrap();
    assert_eq!(
        ToolCatalog::new([first.clone(), first]).unwrap_err().code(),
        PreparationErrorCode::InvalidToolDefinition
    );
    assert_eq!(
        ExecutionRequirements::new([], 0, 1).unwrap_err().code(),
        PreparationErrorCode::InvalidToolDefinition
    );
}

#[test]
fn executable_meaning_changes_digest() {
    let fixture = fixture();
    let base = fixture["definitions"][0].clone();
    let intent = ToolIntent::new("call", "read_file", r#"{"path":"a"}"#);
    let digest = |value: Value| {
        ToolCatalog::new([definition(&value).unwrap()])
            .unwrap()
            .prepare(&intent)
            .unwrap()
            .input_digest()
            .to_owned()
    };
    let original = digest(base.clone());

    let mut revision = base.clone();
    revision["revision"] = Value::String("2".into());
    assert_ne!(original, digest(revision));

    let mut requirements = base.clone();
    requirements["requirements"]["max_output_bytes"] = 8192.into();
    assert_ne!(original, digest(requirements));

    let mut replay = base;
    replay["replay_class"] = Value::String("never_replay".into());
    assert_ne!(original, digest(replay));

    let changed_arguments = catalog(&fixture)
        .prepare(&ToolIntent::new("call", "read_file", r#"{"path":"b"}"#))
        .unwrap();
    assert_ne!(original, changed_arguments.input_digest());
}
