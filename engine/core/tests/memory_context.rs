use std::{fs, path::PathBuf};

use garive_core::{
    derive_context_with_memory, ContextPurpose, ContextRequest, FactRef, MemoryContextError,
    MemoryContextItem, MemoryContextState, MemoryRecallContextBatch, MemoryRecallProduct,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/memory-context-derive-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn batch(name: &str) -> MemoryRecallContextBatch {
    let (position, product, state, content) = match name {
        "menu" => (
            3,
            MemoryRecallProduct::Menu,
            MemoryContextState::Active,
            None,
        ),
        "archived-menu" => (
            3,
            MemoryRecallProduct::Menu,
            MemoryContextState::Archived,
            None,
        ),
        "detail" => (
            4,
            MemoryRecallProduct::Detail,
            MemoryContextState::Active,
            Some("Use metric units.".into()),
        ),
        "detail-2" => (
            5,
            MemoryRecallProduct::Detail,
            MemoryContextState::Cold,
            Some("Use metric units.".into()),
        ),
        _ => panic!("unknown batch"),
    };
    MemoryRecallContextBatch {
        fact_ref: FactRef {
            session_id: "session".into(),
            position,
        },
        fact_id: format!("fact-{position}"),
        payload_digest: "a".repeat(64),
        selection_id: format!("selection-{position}"),
        request_digest: "b".repeat(64),
        namespace_id: "user".into(),
        product,
        selection_policy_revision: "baseline-v1".into(),
        through_position: 2,
        truncated: false,
        items: vec![MemoryContextItem {
            record_id: format!("record-{position}"),
            revision_id: "revision-1".into(),
            memory_type: "semantic".into(),
            role: "preference".into(),
            authority: "user_declared".into(),
            state,
            safe_label: "unit preference".into(),
            content_digest: "dd407b2b50d5735761059db743e2d628f0f6b17585ec025089e82380986dcff9"
                .into(),
            content_byte_length: 17,
            content_utf8: content,
        }],
    }
}

#[test]
fn rust_consumes_every_memory_context_case() {
    let document = fixture();
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let purpose = match case["purpose"].as_str().unwrap() {
            "inference" => ContextPurpose::Inference,
            "governance" => ContextPurpose::Governance,
            _ => panic!("unknown purpose"),
        };
        let request = ContextRequest {
            session_id: "session".into(),
            turn_id: "turn".into(),
            purpose,
            after_position: None,
            through_position: 5,
            max_items: 8,
            max_utf8_bytes: case["max_bytes"].as_u64().unwrap() as usize,
        };
        let batches = case["batches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| batch(value.as_str().unwrap()))
            .collect::<Vec<_>>();
        match derive_context_with_memory(&request, &[], &batches) {
            Ok(surface) => {
                assert_eq!(case["status"], "ok", "{}", case["name"]);
                assert_eq!(
                    positions(&surface.retained_refs),
                    numbers(&case["retained"])
                );
                assert_eq!(positions(&surface.dropped_refs), numbers(&case["dropped"]));
                assert_eq!(
                    positions(&surface.filtered_refs),
                    numbers(&case["filtered"])
                );
                assert!(surface.items.iter().all(|item| match item {
                    garive_core::ContextItem::Input { kind, .. } =>
                        *kind == garive_core::CandidateKind::Memory,
                    _ => false,
                }));
            }
            Err(error) => assert_eq!(error_code(error), case["status"], "{}", case["name"]),
        }
    }
}

fn positions(values: &[FactRef]) -> Vec<u64> {
    values.iter().map(|value| value.position).collect()
}
fn numbers(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_u64().unwrap())
        .collect()
}
fn error_code(value: MemoryContextError) -> &'static str {
    match value {
        MemoryContextError::InvalidBinding => "invalid-binding",
        MemoryContextError::DuplicateRecall => "duplicate-recall",
        MemoryContextError::Context(code) => code.0,
    }
}
