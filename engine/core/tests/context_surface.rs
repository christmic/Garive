use std::{collections::BTreeSet, fs, path::PathBuf};

use garive_core::{
    derive_context, CandidateKind, ContextCandidate, ContextDerivationError, ContextItem,
    ContextPurpose, ContextRequest, FactRef, Retention, Visibility,
};
use garive_llm::{MediaKind, ModelInputContent, ModelInputItem, ModelRole};
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

fn kind(value: Option<&str>) -> CandidateKind {
    match value.unwrap_or("user-input") {
        "instruction" => CandidateKind::Instruction,
        "user-input" => CandidateKind::UserInput,
        "model-output" => CandidateKind::ModelOutput,
        "tool-observation" => CandidateKind::ToolObservation,
        "approval" => CandidateKind::Approval,
        "summary" => CandidateKind::Summary,
        "system-notice" => CandidateKind::SystemNotice,
        other => panic!("unknown candidate kind: {other}"),
    }
}

fn candidates(values: &Value) -> Vec<ContextCandidate> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| ContextCandidate {
            fact_ref: FactRef {
                session_id: value["session"].as_str().unwrap_or("session-1").into(),
                position: value["position"].as_u64().unwrap(),
            },
            kind: kind(value["kind"].as_str()),
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
    assert_eq!(cases.len(), 12);
    for case in cases {
        let request = request(&case["request"]);
        let expected = &case["expected"];
        let result = derive_context(&request, &candidates(&case["candidates"]));
        if expected["status"] != "ok" {
            let error = result.unwrap_err();
            assert_eq!(error.code(), expected["status"], "{}", case["name"]);
            continue;
        }
        let surface = result.unwrap();
        assert_eq!(surface.purpose, request.purpose, "{}", case["name"]);
        assert_eq!(
            surface.from_position,
            request.after_position.unwrap_or(0) + 1,
            "{}",
            case["name"]
        );
        assert_eq!(
            surface.through_position, request.through_position,
            "{}",
            case["name"]
        );
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
    let base = request(&fixture()["cases"][0]["request"]);
    let invalid_requests = [
        ContextRequest {
            session_id: String::new(),
            ..base.clone()
        },
        ContextRequest {
            turn_id: String::new(),
            ..base.clone()
        },
        ContextRequest {
            through_position: 0,
            ..base.clone()
        },
        ContextRequest {
            max_items: 0,
            ..base.clone()
        },
        ContextRequest {
            max_utf8_bytes: 0,
            ..base.clone()
        },
        ContextRequest {
            after_position: Some(base.through_position),
            ..base.clone()
        },
        ContextRequest {
            after_position: Some(u64::MAX),
            ..base.clone()
        },
    ];
    for request in invalid_requests {
        assert_eq!(
            derive_context(&request, &[]),
            Err(ContextDerivationError::InvalidRequest)
        );
    }

    let candidate =
        |session_id: &str, position: u64, items: Vec<ModelInputItem>| ContextCandidate {
            fact_ref: FactRef {
                session_id: session_id.into(),
                position,
            },
            kind: CandidateKind::Instruction,
            retention: Retention::Required,
            visibility: Visibility::Visible,
            items,
        };
    let text = || {
        vec![ModelInputItem::Message {
            role: ModelRole::System,
            content: vec![ModelInputContent::Text("instruction".into())],
        }]
    };
    assert_eq!(
        derive_context(&base, &[candidate("other", 2, text())]),
        Err(ContextDerivationError::SessionMismatch)
    );
    assert_eq!(
        derive_context(&base, &[candidate("session-1", 0, text())]),
        Err(ContextDerivationError::PositionBeyondSurface)
    );
    assert_eq!(
        derive_context(&base, &[candidate("session-1", 5, text())]),
        Err(ContextDerivationError::PositionBeyondSurface)
    );
    assert_eq!(
        derive_context(&base, &[candidate("session-1", 2, Vec::new())]),
        Err(ContextDerivationError::EmptyRequiredContent)
    );
    let mut empty_visibility = candidate("session-1", 2, text());
    empty_visibility.visibility = Visibility::Purposes(BTreeSet::new());
    assert_eq!(
        derive_context(&base, &[empty_visibility]),
        Err(ContextDerivationError::InvalidVisibility)
    );
}

#[test]
fn every_model_input_payload_field_counts_toward_the_budget() {
    let request = ContextRequest {
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        purpose: ContextPurpose::Inference,
        after_position: None,
        through_position: 1,
        max_items: 3,
        max_utf8_bytes: 31,
    };
    let candidate = ContextCandidate {
        fact_ref: FactRef {
            session_id: "session-1".into(),
            position: 1,
        },
        kind: CandidateKind::Instruction,
        retention: Retention::Required,
        visibility: Visibility::Visible,
        items: vec![
            ModelInputItem::Message {
                role: ModelRole::System,
                content: vec![
                    ModelInputContent::Text("a".into()),
                    ModelInputContent::MediaReference {
                        media_kind: MediaKind::Other("custom".into()),
                        reference: "ref".into(),
                        media_type: "image/png".into(),
                    },
                ],
            },
            ModelInputItem::ToolObservation {
                model_call_id: "call".into(),
                result_json: "{}".into(),
            },
            ModelInputItem::ReasoningReference {
                reference: "reason".into(),
            },
        ],
    };
    let surface = derive_context(&request, &[candidate]).unwrap();
    assert_eq!(surface.item_count, 3);
    assert_eq!(surface.utf8_bytes, 31);
}
