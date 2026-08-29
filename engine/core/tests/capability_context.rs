use std::{collections::BTreeSet, fs, path::PathBuf};

use garive_core::{
    assemble_model_inputs, derive_context, merge_context_candidates, CandidateKind,
    ContextCandidate, ContextItem, ContextPurpose, ContextRequest, ContextSurface, FactRef,
    Retention, Visibility,
};
use garive_llm::{ModelInputContent, ModelInputItem, ModelRole};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/capability-context-admission-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn candidate(position: u64) -> ContextCandidate {
    let (kind, retention, text) = match position {
        1 => (CandidateKind::UserInput, Retention::Required, "input"),
        2 => (CandidateKind::Skill, Retention::Required, "skill"),
        3 => (CandidateKind::Memory, Retention::Optional, "memory"),
        4 => (CandidateKind::Knowledge, Retention::Optional, "knowledge"),
        _ => panic!("unknown position"),
    };
    ContextCandidate {
        fact_ref: FactRef {
            session_id: "session".into(),
            position,
        },
        kind,
        retention,
        visibility: Visibility::Purposes(BTreeSet::from([ContextPurpose::Inference])),
        items: vec![ModelInputItem::Message {
            role: if kind == CandidateKind::Skill {
                ModelRole::Developer
            } else {
                ModelRole::User
            },
            content: vec![ModelInputContent::Text(text.into())],
        }],
    }
}

#[test]
fn rust_consumes_every_capability_context_case() {
    let document = fixture();
    for case in document["merge_cases"].as_array().unwrap() {
        let base = candidates(&case["base"]);
        let capability = candidates(&case["capability"]);
        match merge_context_candidates(base, &capability) {
            Ok(values) => assert_eq!(
                positions(&values),
                numbers(&case["merged"]),
                "{}",
                case["name"]
            ),
            Err(error) => assert_eq!(
                error.code(),
                case["status"].as_str().unwrap(),
                "{}",
                case["name"]
            ),
        }
    }
    let case = &document["budget_case"];
    let merged =
        merge_context_candidates(candidates(&case["base"]), &candidates(&case["capability"]))
            .unwrap();
    let surface = derive_context(
        &ContextRequest {
            session_id: "session".into(),
            turn_id: "turn".into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: 4,
            max_items: case["max_items"].as_u64().unwrap() as usize,
            max_utf8_bytes: case["max_bytes"].as_u64().unwrap() as usize,
        },
        &merged,
    )
    .unwrap();
    assert_eq!(refs(&surface.retained_refs), numbers(&case["retained"]));
    assert_eq!(refs(&surface.dropped_refs), numbers(&case["dropped"]));
    let kinds = surface
        .items
        .iter()
        .filter_map(|item| match item {
            ContextItem::Input { kind, .. } => Some(kind_name(*kind)),
            ContextItem::RedactedItem { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds, strings(&case["item_kinds"]));

    let case = &document["assembly_case"];
    let items = case["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| ContextItem::Input {
            fact_ref: FactRef {
                session_id: "session".into(),
                position: value["position"].as_u64().unwrap(),
            },
            kind: fixture_kind(value["kind"].as_str().unwrap()),
            item: ModelInputItem::Message {
                role: fixture_role(value["role"].as_str().unwrap()),
                content: vec![ModelInputContent::Text(
                    value["text"].as_str().unwrap().into(),
                )],
            },
        })
        .collect::<Vec<_>>();
    let surface = ContextSurface {
        purpose: ContextPurpose::Inference,
        from_position: 1,
        through_position: items.len() as u64,
        item_count: items.len(),
        utf8_bytes: 1,
        items,
        retained_refs: vec![],
        dropped_refs: vec![],
        filtered_refs: vec![],
    };
    let texts = assemble_model_inputs(surface)
        .into_iter()
        .map(|item| match item {
            ModelInputItem::Message { content, .. } => match &content[0] {
                ModelInputContent::Text(text) => text.clone(),
                _ => panic!("fixture text"),
            },
            _ => panic!("fixture message"),
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, strings(case.get("expected_texts").unwrap()));
}

fn fixture_kind(value: &str) -> CandidateKind {
    match value {
        "instruction" => CandidateKind::Instruction,
        "user_input" => CandidateKind::UserInput,
        "skill" => CandidateKind::Skill,
        "memory" => CandidateKind::Memory,
        "knowledge" => CandidateKind::Knowledge,
        "system_notice" => CandidateKind::SystemNotice,
        "model_output" => CandidateKind::ModelOutput,
        _ => panic!("unknown kind"),
    }
}

fn fixture_role(value: &str) -> ModelRole {
    match value {
        "system" => ModelRole::System,
        "developer" => ModelRole::Developer,
        "user" => ModelRole::User,
        "assistant" => ModelRole::Assistant,
        _ => panic!("unknown role"),
    }
}

fn candidates(value: &Value) -> Vec<ContextCandidate> {
    numbers(value).into_iter().map(candidate).collect()
}
fn positions(values: &[ContextCandidate]) -> Vec<u64> {
    values.iter().map(|v| v.fact_ref.position).collect()
}
fn refs(values: &[FactRef]) -> Vec<u64> {
    values.iter().map(|v| v.position).collect()
}
fn numbers(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect()
}
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}
fn kind_name(value: CandidateKind) -> &'static str {
    match value {
        CandidateKind::UserInput => "user_input",
        CandidateKind::Skill => "skill",
        CandidateKind::Knowledge => "knowledge",
        _ => "unexpected",
    }
}
