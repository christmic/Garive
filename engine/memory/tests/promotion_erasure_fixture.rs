use std::{fs, path::PathBuf};

use garive_memory::{
    complete_memory_promotion, record_memory_erasure, request_memory_promotion,
    DurableFactReference, ErasureDisposition, ErasureTargetKind, ErasureTargetStatus,
    EvidenceTally, HypothesisState, MemoryErasureRequest, MemoryErasureTarget,
    MemoryErasureTargetResult, MemoryLifecycle, MemoryPromotionPolicy, MemoryPromotionReceipt,
    MemoryType,
};
use serde_json::Value;

#[test]
fn shared_promotion_policy_admits_only_evidenced_active_or_cold_memory() {
    let root = fixture();
    let policy = promotion_policy(&root["promotion_policy"]);
    for case in root["promotion_cases"].as_array().unwrap() {
        let lifecycle = lifecycle(case);
        let result = request_memory_promotion(
            "promotion-request",
            "namespace",
            "record",
            "revision",
            memory_type(case["type"].as_str().unwrap()),
            &lifecycle,
            number(case, "helpful_uses"),
            &policy,
            "knowledge-proposal",
            "e".repeat(64),
        );
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                result.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let request = result.unwrap();
            assert_eq!(request.request_id(), "promotion-request");
            assert_eq!(request.policy_revision(), policy.revision());
            assert_eq!(case["expected"], "requested");
        }
    }
}

#[test]
fn shared_promotion_receipts_bind_the_exact_request_and_transition() {
    let root = fixture();
    let policy = promotion_policy(&root["promotion_policy"]);
    for case in root["promotion_receipt_cases"].as_array().unwrap() {
        let lifecycle = MemoryLifecycle::new(
            HypothesisState::Active,
            EvidenceTally {
                verified: 3,
                falsified: 0,
                neutral: 0,
            },
            20,
            None,
        )
        .unwrap();
        let request = request_memory_promotion(
            case["request_id"].as_str().unwrap(),
            "namespace",
            "record",
            "revision",
            MemoryType::Lesson,
            &lifecycle,
            2,
            &policy,
            case["proposal_id"].as_str().unwrap(),
            "e".repeat(64),
        )
        .unwrap();
        let receipt = MemoryPromotionReceipt::new(
            case["receipt_request_id"].as_str().unwrap(),
            case["receipt_proposal_id"].as_str().unwrap(),
            "knowledge-record",
            "knowledge-revision",
            "f".repeat(64),
        )
        .unwrap();
        let result =
            complete_memory_promotion(&request, &receipt, &lifecycle, number(case, "position"));
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                result.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let promoted = result.unwrap();
            assert_eq!(promoted.state(), HypothesisState::Promoted);
            assert_eq!(
                promoted.promoted_knowledge_receipt_digest(),
                Some(receipt.receipt_digest())
            );
        }
    }
}

#[test]
fn shared_erasure_receipts_cover_every_target_and_never_hide_pending_work() {
    let root = fixture();
    let request = erasure_request(&root);
    for case in root["erasure_cases"].as_array().unwrap() {
        let results = case["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(erasure_result)
            .collect::<Result<Vec<_>, _>>();
        let result = results.and_then(|results| {
            record_memory_erasure(
                &request,
                format!("attempt-{}", case["name"].as_str().unwrap()),
                number(case, "attempted_at_position"),
                results,
            )
        });
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                result.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let receipt = result.unwrap();
            let actual = match receipt.disposition() {
                ErasureDisposition::Complete => "complete",
                ErasureDisposition::Partial => "partial",
            };
            assert_eq!(
                actual,
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(receipt.results().len(), request.targets().len());
        }
    }
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/memory-promotion-erasure-v1.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn promotion_policy(value: &Value) -> MemoryPromotionPolicy {
    MemoryPromotionPolicy::new(
        value["revision"].as_str().unwrap(),
        value["allowed_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| memory_type(item.as_str().unwrap()))
            .collect(),
        number(value, "min_verified"),
        number(value, "max_falsified"),
        number(value, "min_helpful_uses"),
    )
    .unwrap()
}

fn lifecycle(value: &Value) -> MemoryLifecycle {
    MemoryLifecycle::new(
        state(value["state"].as_str().unwrap()),
        EvidenceTally {
            verified: number(value, "verified"),
            falsified: number(value, "falsified"),
            neutral: number(value, "neutral"),
        },
        number(value, "last_position"),
        None,
    )
    .unwrap()
}

fn erasure_request(root: &Value) -> MemoryErasureRequest {
    let fact = &root["tombstone_fact"];
    MemoryErasureRequest::new(
        "erasure-request",
        "namespace",
        "record",
        "revision",
        DurableFactReference::new(
            fact["session_id"].as_str().unwrap(),
            number(fact, "position"),
            fact["fact_id"].as_str().unwrap(),
            fact["payload_digest"].as_str().unwrap(),
        )
        .unwrap(),
        "erasure-v1",
        root["erasure_targets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                MemoryErasureTarget::new(
                    value["target_id"].as_str().unwrap(),
                    target_kind(value["kind"].as_str().unwrap()),
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}

fn erasure_result(value: &Value) -> Result<MemoryErasureTargetResult, garive_memory::MemoryError> {
    MemoryErasureTargetResult::new(
        value["target_id"].as_str().unwrap(),
        match value["status"].as_str().unwrap() {
            "erased" => ErasureTargetStatus::Erased,
            "not_present" => ErasureTargetStatus::NotPresent,
            "pending_backup_retention" => ErasureTargetStatus::PendingBackupRetention,
            "pending_retry" => ErasureTargetStatus::PendingRetry,
            other => panic!("unknown status: {other}"),
        },
        value["receipt_digest"].as_str().unwrap(),
        value
            .get("not_before_position")
            .map(|_| number(value, "not_before_position")),
    )
}

fn target_kind(value: &str) -> ErasureTargetKind {
    match value {
        "primary_store" => ErasureTargetKind::PrimaryStore,
        "projection" => ErasureTargetKind::Projection,
        "cache" => ErasureTargetKind::Cache,
        "backup" => ErasureTargetKind::Backup,
        other => panic!("unknown target kind: {other}"),
    }
}

fn memory_type(value: &str) -> MemoryType {
    match value {
        "semantic" => MemoryType::Semantic,
        "episodic" => MemoryType::Episodic,
        "lesson" => MemoryType::Lesson,
        "procedural" => MemoryType::Procedural,
        other => panic!("unknown memory type: {other}"),
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
