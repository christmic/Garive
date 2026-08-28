use std::{collections::BTreeSet, fs, path::PathBuf};

use garive_core::{
    derive_context, CandidateKind, ContextCandidate, ContextDerivationError, ContextItem,
    ContextPurpose, ContextRequest, FactRef, Retention, Visibility,
};
use garive_llm::{ModelInputContent, ModelInputItem, ModelRole};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/context-surface.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn purpose(value: &str) -> ContextPurpose {
    match value {
        "inference" => ContextPurpose::Inference,
        "governance" => ContextPurpose::Governance,
        "tool-preparation" => ContextPurpose::ToolPreparation,
        "summarization" => ContextPurpose::Summarization,
        other => panic!("unknown purpose: {other}"),
    }
}

fn request(value: &Value) -> ContextRequest {
    ContextRequest {
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        purpose: purpose(value["purpose"].as_str().unwrap()),
        after_position: value["after"].as_u64(),
        through_position: value["through"].as_u64().unwrap(),
        max_items: value["max_items"].as_u64().unwrap() as usize,
        max_utf8_bytes: value["max_bytes"].as_u64().unwrap() as usize,
    }
}

fn visibility(value: &str) -> Visibility {
    match value {
        "visible" => Visibility::Visible,
        "redacted" => Visibility::Redacted,
        limited if limited.starts_with("purpose:") => {
            Visibility::Purposes(BTreeSet::from([purpose(
                limited.strip_prefix("purpose:").unwrap(),
            )]))
        }
        other => panic!("unknown visibility: {other}"),
    }
}

fn candidates(values: &Value) -> Vec<ContextCandidate> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| ContextCandidate {
            fact_ref: FactRef {
                session_id: "session-1".into(),
                position: value["position"].as_u64().unwrap(),
            },
            kind: CandidateKind::UserInput,
            retention: match value["retention"].as_str().unwrap() {
                "required" => Retention::Required,
                "optional" => Retention::Optional,
                other => panic!("unknown retention: {other}"),
            },
            visibility: visibility(value["visibility"].as_str().unwrap()),
            items: value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| ModelInputItem::Message {
                    role: ModelRole::User,
                    content: vec![ModelInputContent::Text(item.as_str().unwrap().into())],
                })
                .collect(),
        })
        .collect()
}

fn positions(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect()
}

fn rendered_items(items: &[ContextItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            ContextItem::Input {
                item: ModelInputItem::Message { content, .. },
                ..
            } => match &content[0] {
                ModelInputContent::Text(text) => format!("text:{text}"),
                _ => panic!("unexpected media fixture"),
            },
            ContextItem::RedactedItem { .. } => "redacted".into(),
            _ => panic!("unexpected fixture item"),
        })
        .collect()
}

#[test]
fn rust_consumes_every_context_case() {
    let document = fixture();
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let expected = &case["expected"];
        let result = derive_context(&request(&case["request"]), &candidates(&case["candidates"]));
        if expected["status"] != "ok" {
            let error = result.unwrap_err();
            assert_eq!(error.code(), expected["status"], "{}", case["name"]);
            continue;
        }
        let surface = result.unwrap();
        assert_eq!(
            surface
                .retained_refs
                .iter()
                .map(|r| r.position)
                .collect::<Vec<_>>(),
            positions(&expected["retained"])
        );
        assert_eq!(
            surface
                .dropped_refs
                .iter()
                .map(|r| r.position)
                .collect::<Vec<_>>(),
            positions(&expected["dropped"])
        );
        assert_eq!(
            surface
                .filtered_refs
                .iter()
                .map(|r| r.position)
                .collect::<Vec<_>>(),
            positions(&expected["filtered"])
        );
        assert_eq!(
            rendered_items(&surface.items),
            expected["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            surface.item_count,
            expected["item_count"].as_u64().unwrap() as usize
        );
        assert_eq!(
            surface.utf8_bytes,
            expected["bytes"].as_u64().unwrap() as usize
        );
    }
}

#[test]
fn request_and_candidate_boundaries_fail_closed() {
    let mut value = request(&fixture()["cases"][0]["request"]);
    value.through_position = 0;
    assert_eq!(
        derive_context(&value, &[]).unwrap_err(),
        ContextDerivationError::InvalidRequest
    );
}
