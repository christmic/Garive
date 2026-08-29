use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_memory::{
    record_memory_erasure, request_memory_promotion, AdmissionAssessment, CandidateStability,
    ContentBinding, DurableFactReference, ErasureTargetKind, ErasureTargetStatus, EvidenceTally,
    HypothesisState, MemoryAuthority, MemoryAuthorityBinding, MemoryCandidate,
    MemoryCandidateIntent, MemoryCandidateSource, MemoryErasureRequest, MemoryErasureTarget,
    MemoryErasureTargetResult, MemoryKind, MemoryLifecycle, MemoryMaintenanceDecision,
    MemoryPromotionPolicy, MemoryPromotionReceipt, MemoryRecord, MemoryScope, MemoryScopeBinding,
    MemoryScopeClass, MemorySensitivity, MemoryState, MemoryStatus, MemoryTombstone, MemoryType,
};
use garive_runtime::{
    plan_memory_erasure_receipt, plan_memory_forget, plan_memory_maintenance_decision,
    plan_memory_promotion_receipt, plan_memory_promotion_request, plan_memory_tombstone,
    MemoryMaintenanceContext, MemoryTombstoneContext, MemoryTombstoneReason, SqliteLedger,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn maintenance_promotion_and_erasure_batches_survive_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory-maintenance.sqlite3");
    let session = SessionId::try_from("privacy-session").unwrap();
    let context = MemoryMaintenanceContext {
        session_id: session.clone(),
        namespace_id: "namespace".into(),
        recorded_at: "2026-08-30T00:00:00Z".into(),
    };
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![session_opened()])
        .unwrap();

    let candidate = candidate();
    let assessment =
        AdmissionAssessment::new(true, CandidateStability::Confirmed, None, None).unwrap();
    let decision =
        garive_memory::decide_candidate(&candidate, Some(&assessment), "proposal").unwrap();
    assert!(matches!(decision, MemoryMaintenanceDecision::Add { .. }));
    let facts = plan_memory_maintenance_decision(
        &context,
        &candidate,
        &decision,
        &"a".repeat(64),
        &"b".repeat(64),
    )
    .unwrap();
    ledger.commit(session.clone(), 1, facts).unwrap();

    let lifecycle = MemoryLifecycle::new(
        HypothesisState::Active,
        EvidenceTally {
            verified: 3,
            falsified: 0,
            neutral: 0,
        },
        2,
        None,
    )
    .unwrap();
    let policy =
        MemoryPromotionPolicy::new("promotion-v1", vec![MemoryType::Lesson], 3, 0, 2).unwrap();
    let request = request_memory_promotion(
        "promotion-request",
        "namespace",
        "record",
        "revision",
        MemoryType::Lesson,
        &lifecycle,
        2,
        &policy,
        "knowledge-proposal",
        "c".repeat(64),
    )
    .unwrap();
    let request_fact = plan_memory_promotion_request(&context, &request).unwrap();
    ledger
        .commit(session.clone(), 2, vec![request_fact])
        .unwrap();
    let receipt = MemoryPromotionReceipt::new(
        "promotion-request",
        "knowledge-proposal",
        "knowledge-record",
        "knowledge-revision",
        "d".repeat(64),
    )
    .unwrap();
    let promoted =
        plan_memory_promotion_receipt(&context, &request, &receipt, &lifecycle, 4).unwrap();
    assert_eq!(promoted.lifecycle.state(), HypothesisState::Promoted);
    ledger.commit(session.clone(), 3, promoted.facts).unwrap();

    let state = memory_state();
    let tombstone = plan_memory_tombstone(
        &MemoryTombstoneContext {
            command_id: "forget-command".into(),
            recorded_at: context.recorded_at.clone(),
        },
        &state,
        &MemoryTombstone {
            record_id: "record".into(),
            revision_id: "revision".into(),
        },
        MemoryTombstoneReason::UserRequest,
    )
    .unwrap();
    let tombstone_reference = DurableFactReference::new(
        session.as_str(),
        7,
        tombstone.fact.fact_id.as_str(),
        tombstone.fact.payload.sha256(),
    )
    .unwrap();
    let erasure_request = MemoryErasureRequest::new(
        "erasure-request",
        "namespace",
        "record",
        "revision",
        tombstone_reference,
        "erasure-v1",
        erasure_targets(),
    )
    .unwrap();
    let forget = plan_memory_forget(&context, 6, tombstone, &erasure_request).unwrap();
    ledger.commit(session.clone(), 4, forget).unwrap();
    let partial = record_memory_erasure(
        &erasure_request,
        "attempt-1",
        8,
        vec![
            erasure_result("primary", ErasureTargetStatus::Erased, None),
            erasure_result(
                "backup",
                ErasureTargetStatus::PendingBackupRetention,
                Some(20),
            ),
        ],
    )
    .unwrap();
    let partial_fact = plan_memory_erasure_receipt(&context, &erasure_request, &partial).unwrap();
    ledger
        .commit(session.clone(), 5, vec![partial_fact])
        .unwrap();
    drop(ledger);

    let restarted = SqliteLedger::open(&path).unwrap();
    let facts = restarted.read_facts(&session, 1, 9, None).unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "memory.candidate_recorded",
            "memory.maintenance_decided",
            "memory.promotion_requested",
            "memory.promotion_recorded",
            "memory.lifecycle_transitioned",
            "memory.tombstoned",
            "memory.erasure_requested",
            "memory.erasure_recorded",
        ],
    );
    assert!(facts
        .iter()
        .all(|fact| fact.turn_id.is_none() && fact.execution_id.is_none()));
}

fn session_opened() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("session-opened").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({})).unwrap(),
        recorded_at: "2026-08-30T00:00:00Z".into(),
    }
}

fn candidate() -> MemoryCandidate {
    MemoryCandidate::new(
        "candidate",
        "namespace",
        "extractor-v1",
        MemoryCandidateSource::SessionEnd,
        MemoryCandidateIntent::Learn {
            memory_type: MemoryType::Lesson,
            role: MemoryKind::LearnedFact,
            authority: MemoryAuthorityBinding::new(MemoryAuthority::AgentLearned, None).unwrap(),
            scope: MemoryScopeBinding::new(MemoryScopeClass::User, None).unwrap(),
            content: ContentBinding::from_inline("lesson"),
            content_bytes: 6,
            evidence: vec![DurableFactReference::new(
                "privacy-session",
                1,
                "source",
                "e".repeat(64),
            )
            .unwrap()],
        },
    )
    .unwrap()
}

fn memory_state() -> MemoryState {
    MemoryState::new(vec![MemoryRecord::new(
        "record",
        "revision",
        "namespace",
        MemoryScope::Namespace,
        MemoryKind::LearnedFact,
        ContentBinding::from_inline("lesson"),
        vec![DurableFactReference::new("privacy-session", 1, "source", "e".repeat(64)).unwrap()],
        MemoryStatus::Active,
        MemorySensitivity::Ordinary,
        9_000,
        2,
        None,
        None,
    )
    .unwrap()])
    .unwrap()
}

fn erasure_targets() -> Vec<MemoryErasureTarget> {
    vec![
        MemoryErasureTarget::new("primary", ErasureTargetKind::PrimaryStore).unwrap(),
        MemoryErasureTarget::new("backup", ErasureTargetKind::Backup).unwrap(),
    ]
}

fn erasure_result(
    target_id: &str,
    status: ErasureTargetStatus,
    not_before_position: Option<u64>,
) -> MemoryErasureTargetResult {
    MemoryErasureTargetResult::new(target_id, status, "f".repeat(64), not_before_position).unwrap()
}
