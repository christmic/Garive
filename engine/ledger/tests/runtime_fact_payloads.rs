use std::{fs, path::PathBuf};

use garive_ledger::{
    validate_runtime_fact, CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, LedgerError,
    ModelRequestId, RuntimeFactDisposition, ToolInvocationId, TurnId,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger/runtime-facts-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn fact(case: &Value, schema_version: u32) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("fact").unwrap(),
        turn_id: case["turn"]
            .as_str()
            .map(|value| TurnId::try_from(value).unwrap()),
        execution_id: case["execution"]
            .as_str()
            .map(|value| ExecutionId::try_from(value).unwrap()),
        model_request_id: case["request"]
            .as_str()
            .map(|value| ModelRequestId::try_from(value).unwrap()),
        tool_invocation_id: case["tool"]
            .as_str()
            .map(|value| ToolInvocationId::try_from(value).unwrap()),
        kind: FactKind::new(case["kind"].as_str().unwrap()).unwrap(),
        schema_version,
        payload: CanonicalPayload::from_value(&case["payload"]).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn with_payload(mut fact: FactDraft, payload: Value) -> FactDraft {
    fact.payload = CanonicalPayload::from_value(&payload).unwrap();
    fact
}

#[test]
fn every_c6_payload_fixture_is_applied_as_v1() {
    let fixture = fixture();
    let cases = fixture["valid_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 35);
    for case in cases {
        assert_eq!(
            validate_runtime_fact(&fact(case, 1)),
            Ok(RuntimeFactDisposition::AppliedV1),
            "{}",
            case["kind"]
        );
    }
}

#[test]
fn exact_fields_types_and_envelopes_fail_closed_for_every_kind() {
    for case in fixture()["valid_cases"].as_array().unwrap() {
        let original = fact(case, 1);
        let mut missing = case["payload"].clone();
        let first = missing.as_object().unwrap().keys().next().unwrap().clone();
        missing.as_object_mut().unwrap().remove(&first);
        assert_eq!(
            validate_runtime_fact(&with_payload(original.clone(), missing)),
            Err(LedgerError::InvalidFact),
            "missing field: {}",
            case["kind"]
        );

        let mut extra = case["payload"].clone();
        extra
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), Value::Bool(true));
        assert_eq!(
            validate_runtime_fact(&with_payload(original.clone(), extra)),
            Err(LedgerError::InvalidFact),
            "extra field: {}",
            case["kind"]
        );

        let mut wrong = case["payload"].clone();
        wrong.as_object_mut().unwrap().insert(first, Value::Null);
        assert_eq!(
            validate_runtime_fact(&with_payload(original.clone(), wrong)),
            Err(LedgerError::InvalidFact),
            "wrong type: {}",
            case["kind"]
        );

        let mut missing_identity = original;
        if missing_identity.tool_invocation_id.take().is_none()
            && missing_identity.model_request_id.take().is_none()
            && missing_identity.execution_id.take().is_none()
        {
            missing_identity.turn_id = None;
        }
        assert_eq!(
            validate_runtime_fact(&missing_identity),
            Err(LedgerError::InvalidFact),
            "missing envelope identity: {}",
            case["kind"]
        );
    }
}

#[test]
fn malformed_digests_and_inline_content_mismatches_are_rejected() {
    let mut digest_cases = 0;
    for case in fixture()["valid_cases"].as_array().unwrap() {
        let mut payload = case["payload"].clone();
        if corrupt_first_digest(&mut payload) {
            digest_cases += 1;
            assert_eq!(
                validate_runtime_fact(&with_payload(fact(case, 1), payload)),
                Err(LedgerError::InvalidFact),
                "{}",
                case["kind"]
            );
        }
    }
    assert_eq!(digest_cases, 28);
}

#[test]
fn unknown_kinds_and_newer_schemas_remain_opaque() {
    let fixture = fixture();
    let case = &fixture["unknown_schema"];
    assert_eq!(
        validate_runtime_fact(&fact(case, case["schema_version"].as_u64().unwrap() as u32)),
        Ok(RuntimeFactDisposition::Opaque)
    );
    let mut unknown = fact(&fixture["valid_cases"][0], 1);
    unknown.kind = FactKind::new("future.runtime_fact").unwrap();
    assert_eq!(
        validate_runtime_fact(&unknown),
        Ok(RuntimeFactDisposition::Opaque)
    );
}

fn corrupt_first_digest(value: &mut Value) -> bool {
    match value {
        Value::Object(object) => {
            if let Some(digest) = object.get_mut("digest") {
                *digest = Value::String("ABC".into());
                true
            } else {
                for (key, child) in object {
                    if key.ends_with("_digest") {
                        *child = Value::String("ABC".into());
                        return true;
                    }
                    if corrupt_first_digest(child) {
                        return true;
                    }
                }
                false
            }
        }
        Value::Array(values) => values.iter_mut().any(corrupt_first_digest),
        _ => false,
    }
}
