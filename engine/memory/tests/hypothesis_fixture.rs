use std::{fs, path::PathBuf};

use garive_memory::{
    import_m0_classification, select_recall, EvidenceTally, HypothesisState, LifecycleEvent,
    MemoryAuthority, MemoryAuthorityBinding, MemoryErrorCode, MemoryKind, MemoryLifecycle,
    MemoryRecallCandidate, MemoryScopeBinding, MemoryScopeClass, MemoryType, MemoryTypeDescriptor,
    MemoryTypeRegistry, RecallExploration, RecallProduct, RecallScore, RecallSelectionKind,
    RecallSelectionRequest,
};
use serde_json::Value;

#[test]
fn shared_registry_and_m0_imports_are_exact() {
    let root = fixture();
    let registry = registry(&root);
    for case in root["imports"].as_array().unwrap() {
        let authority = MemoryAuthorityBinding::new(
            authority(case["authority"].as_str().unwrap()),
            case.get("receipt_digest")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
        .unwrap();
        let imported = import_m0_classification(kind(case["m0_kind"].as_str().unwrap()), authority);
        assert_eq!(
            imported.memory_type,
            memory_type(case["expected_type"].as_str().unwrap())
        );
        assert_eq!(imported.role, kind(case["expected_role"].as_str().unwrap()));
        assert!(registry.admits(
            imported.memory_type,
            imported.role,
            imported.authority.authority()
        ));
    }
}

#[test]
fn shared_invalid_authority_scope_and_pair_fail_closed() {
    let root = fixture();
    let registry = registry(&root);
    for case in root["invalid"].as_array().unwrap() {
        let expected = case["expected"].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "user_without_receipt" | "agent_with_receipt" => MemoryAuthorityBinding::new(
                authority(case["authority"].as_str().unwrap()),
                case.get("receipt_digest")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )
            .unwrap_err()
            .code()
            .wire_name(),
            "platform_without_policy" => MemoryScopeBinding::new(MemoryScopeClass::Platform, None)
                .unwrap_err()
                .code()
                .wire_name(),
            "unsupported_pair" => {
                assert!(!registry.admits(
                    memory_type(case["type"].as_str().unwrap()),
                    kind(case["role"].as_str().unwrap()),
                    authority(case["authority"].as_str().unwrap()),
                ));
                MemoryErrorCode::UnknownMemoryType.wire_name()
            }
            other => panic!("unknown invalid case: {other}"),
        };
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}

#[test]
fn registry_requires_complete_canonical_types_and_platform_policy_is_exact() {
    let root = fixture();
    let mut descriptors = descriptors(&root);
    descriptors.swap(0, 1);
    assert_eq!(
        MemoryTypeRegistry::new("r", descriptors)
            .unwrap_err()
            .code(),
        MemoryErrorCode::UnknownMemoryType,
    );
    let digest = "b".repeat(64);
    let platform = MemoryScopeBinding::new(MemoryScopeClass::Platform, Some(digest)).unwrap();
    assert_eq!(platform.scope(), MemoryScopeClass::Platform);
    assert_eq!(
        MemoryScopeBinding::new(MemoryScopeClass::Project, Some("b".repeat(64)))
            .unwrap_err()
            .code(),
        MemoryErrorCode::InvalidMemory,
    );
}

#[test]
fn shared_lifecycle_reduces_exact_tallies_and_failures() {
    let root = fixture();
    for case in root["lifecycle_cases"].as_array().unwrap() {
        let initial = &case["initial"];
        let lifecycle = MemoryLifecycle::new(
            state(initial["state"].as_str().unwrap()),
            EvidenceTally {
                verified: number(initial, "verified"),
                falsified: number(initial, "falsified"),
                neutral: number(initial, "neutral"),
            },
            number(initial, "last_position"),
            None,
        )
        .unwrap();
        let result = lifecycle.apply(event(&case["event"]));
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                result.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let actual = result.unwrap();
            let expected = &case["expected"];
            assert_eq!(
                actual.state(),
                state(expected["state"].as_str().unwrap()),
                "{}",
                case["name"]
            );
            assert_eq!(
                actual.tally(),
                EvidenceTally {
                    verified: number(expected, "verified"),
                    falsified: number(expected, "falsified"),
                    neutral: number(expected, "neutral"),
                }
            );
            if actual.state() == HypothesisState::Promoted {
                assert!(actual.promoted_knowledge_receipt_digest().is_some());
            }
        }
    }
}

#[test]
fn shared_recall_selection_is_bounded_ranked_and_replayable() {
    let root = fixture();
    let candidates: Vec<_> = root["recall_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(recall_candidate)
        .collect();
    for case in root["recall_cases"].as_array().unwrap() {
        let request = recall_request(case);
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                request.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
            continue;
        }
        let request = request.unwrap();
        let first = select_recall(&candidates, &request).unwrap();
        let second = select_recall(&candidates, &request).unwrap();
        assert_eq!(first, second, "{}", case["name"]);
        assert_eq!(
            first
                .items
                .iter()
                .map(|item| item.candidate().record_id())
                .collect::<Vec<_>>(),
            case["expected_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{}",
            case["name"],
        );
        assert_eq!(
            first
                .items
                .iter()
                .map(|item| match item.kind() {
                    RecallSelectionKind::Ranked => "ranked",
                    RecallSelectionKind::Explored => "explored",
                })
                .collect::<Vec<_>>(),
            case["expected_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
        );
        if let Some(draws) = case.get("expected_draws") {
            assert_eq!(
                first
                    .items
                    .iter()
                    .map(|item| item.draw_hex())
                    .collect::<Vec<_>>(),
                draws
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::as_str)
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(first.truncated, case["truncated"].as_bool().unwrap());
    }
    let request = recall_request(&root["recall_cases"][0]).unwrap();
    assert_eq!(
        select_recall(&[candidates[0].clone(), candidates[0].clone()], &request)
            .unwrap_err()
            .code(),
        MemoryErrorCode::InvalidMemory,
    );
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/memory-hypothesis-lifecycle-v1.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn registry(root: &Value) -> MemoryTypeRegistry {
    MemoryTypeRegistry::new(
        root["registry"]["revision"].as_str().unwrap(),
        descriptors(root),
    )
    .unwrap()
}

fn descriptors(root: &Value) -> Vec<MemoryTypeDescriptor> {
    root["registry"]["descriptors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            MemoryTypeDescriptor::new(
                memory_type(value["type"].as_str().unwrap()),
                value["roles"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| kind(v.as_str().unwrap()))
                    .collect(),
                value["authorities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| authority(v.as_str().unwrap()))
                    .collect(),
                value["lifecycle"].as_str().unwrap(),
                value["recall"].as_str().unwrap(),
                value["retention"].as_str().unwrap(),
                value["surface_kind"].as_str().unwrap(),
            )
            .unwrap()
        })
        .collect()
}

fn memory_type(value: &str) -> MemoryType {
    match value {
        "semantic" => MemoryType::Semantic,
        "episodic" => MemoryType::Episodic,
        "lesson" => MemoryType::Lesson,
        "procedural" => MemoryType::Procedural,
        other => panic!("unknown type: {other}"),
    }
}
fn kind(value: &str) -> MemoryKind {
    match value {
        "preference" => MemoryKind::Preference,
        "constraint" => MemoryKind::Constraint,
        "decision" => MemoryKind::Decision,
        "learned_fact" => MemoryKind::LearnedFact,
        "summary" => MemoryKind::Summary,
        other => panic!("unknown role: {other}"),
    }
}
fn authority(value: &str) -> MemoryAuthority {
    match value {
        "user_declared" => MemoryAuthority::UserDeclared,
        "agent_learned" => MemoryAuthority::AgentLearned,
        "organisation_published" => MemoryAuthority::OrganisationPublished,
        other => panic!("unknown authority: {other}"),
    }
}

fn state(value: &str) -> HypothesisState {
    match value {
        "candidate" => HypothesisState::Candidate,
        "active" => HypothesisState::Active,
        "cold" => HypothesisState::Cold,
        "archived" => HypothesisState::Archived,
        "promoted" => HypothesisState::Promoted,
        other => panic!("unknown state: {other}"),
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_str().unwrap().parse().unwrap()
}

fn event(value: &Value) -> LifecycleEvent {
    let position = number(value, "position");
    match value["kind"].as_str().unwrap() {
        "verified" => LifecycleEvent::Verified { position },
        "falsified_in_scope" => LifecycleEvent::Falsified {
            position,
            in_scope: true,
        },
        "falsified_out_of_scope" => LifecycleEvent::Falsified {
            position,
            in_scope: false,
        },
        "neutral" => LifecycleEvent::Neutral { position },
        "cool" => LifecycleEvent::Cool { position },
        "archive" => LifecycleEvent::Archive { position },
        "promote" => LifecycleEvent::Promote {
            position,
            receipt_digest: value
                .get("receipt_digest")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        other => panic!("unknown event: {other}"),
    }
}

fn recall_candidate(value: &Value) -> MemoryRecallCandidate {
    MemoryRecallCandidate::new(
        value["record_id"].as_str().unwrap(),
        value["revision_id"].as_str().unwrap(),
        memory_type(value["type"].as_str().unwrap()),
        kind(value["role"].as_str().unwrap()),
        authority(value["authority"].as_str().unwrap()),
        state(value["state"].as_str().unwrap()),
        value["safe_label"].as_str().unwrap(),
        value["content_digest"].as_str().unwrap(),
        number(value, "content_bytes"),
        number(value, "evidence_count") as u32,
        RecallScore {
            relevance: number(value, "relevance") as u16,
            recency: number(value, "recency") as u16,
            importance: number(value, "importance") as u16,
        },
    )
    .unwrap()
}

fn recall_request(value: &Value) -> Result<RecallSelectionRequest, garive_memory::MemoryError> {
    let exploration = value
        .get("exploration")
        .map(|item| {
            RecallExploration::new(
                item["algorithm"].as_str().unwrap(),
                number(item, "seed"),
                number(item, "slots") as u32,
            )
        })
        .transpose()?;
    RecallSelectionRequest::new(
        if value["product"] == "menu" {
            RecallProduct::Menu
        } else {
            RecallProduct::Detail
        },
        value["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| memory_type(v.as_str().unwrap()))
            .collect(),
        value["roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| kind(v.as_str().unwrap()))
            .collect(),
        value["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| state(v.as_str().unwrap()))
            .collect(),
        "score-sum-v1",
        number(value, "max_items") as u32,
        number(value, "max_bytes"),
        exploration,
    )
}
