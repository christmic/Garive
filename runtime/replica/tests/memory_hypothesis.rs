use std::{collections::BTreeMap, path::PathBuf};

use garive_core::{derive_context_with_memory, ContextPurpose, ContextRequest};

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, SessionId, TurnId,
};
use garive_memory::{
    DurableFactReference, EvidenceTally, HypothesisState, MemoryAuthority, MemoryErrorCode,
    MemoryKind, MemoryLifecycle, MemoryObligation, MemoryObservation, MemoryRecallCandidate,
    MemoryType, ObservationEvidence, ObservationEvidenceKind, ObservationVerdict, RecallProduct,
    RecallScore, RecallSelectionRequest,
};
use garive_runtime::{
    decode_committed_memory_recall, plan_memory_obligation, plan_memory_observation,
    plan_memory_recall, reconstruct_memory_hypothesis_projection, MemoryObligationContext,
    MemoryObservationContext, MemoryPrefix, MemoryRecallContext, SqliteLedger,
};
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn m1_recall_obligation_and_observation_commit_and_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory-m1.sqlite3");
    let session = SessionId::try_from("session-m1").unwrap();
    let turn = TurnId::try_from("turn").unwrap();
    let execution = ExecutionId::try_from("execution").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger.commit(session.clone(), 0, initial_facts()).unwrap();

    let candidate = MemoryRecallCandidate::new(
        "record",
        "revision",
        MemoryType::Semantic,
        MemoryKind::Preference,
        MemoryAuthority::UserDeclared,
        HypothesisState::Active,
        "Language preference",
        "a".repeat(64),
        20,
        1,
        RecallScore {
            relevance: 9_000,
            recency: 8_000,
            importance: 10_000,
        },
    )
    .unwrap();
    let request = RecallSelectionRequest::new(
        RecallProduct::Menu,
        vec![MemoryType::Semantic],
        vec![MemoryKind::Preference],
        vec![HypothesisState::Active],
        "score-sum-v1",
        2,
        64,
        None,
    )
    .unwrap();
    let recall = plan_memory_recall(
        &MemoryRecallContext {
            selection_id: "selection".into(),
            namespace_id: "namespace-a".into(),
            request_digest: "b".repeat(64),
            through_position: 3,
            turn_id: turn.clone(),
            execution_id: execution.clone(),
            recorded_at: "2026-08-30T00:00:01Z".into(),
        },
        &[candidate],
        &request,
    )
    .unwrap();
    assert_eq!(recall.selection.items.len(), 1);
    ledger
        .commit(session.clone(), 1, vec![recall.fact])
        .unwrap();
    let recall_fact = ledger.read_facts(&session, 3, 4, None).unwrap().remove(0);
    let batch = decode_committed_memory_recall(&recall_fact, &BTreeMap::new()).unwrap();
    let surface = derive_context_with_memory(
        &ContextRequest {
            session_id: session.as_str().into(),
            turn_id: turn.as_str().into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: 4,
            max_items: 2,
            max_utf8_bytes: 2_048,
        },
        &[],
        &[batch],
    )
    .unwrap();
    assert_eq!(surface.retained_refs[0].position, 4);
    assert!(decode_committed_memory_recall(
        &recall_fact,
        &BTreeMap::from([(("record".into(), "revision".into()), "hidden".into())]),
    )
    .is_err());

    let application = ledger.read_facts(&session, 0, 1, None).unwrap().remove(0);
    let obligation = MemoryObligation::new(
        "obligation",
        "record",
        "revision",
        DurableFactReference::new(
            session.as_str(),
            1,
            application.fact_id.as_str(),
            application.payload.sha256(),
        )
        .unwrap(),
        "c".repeat(64),
        "d".repeat(64),
        "attribution-v1",
        20,
    )
    .unwrap();
    let obligation_fact = plan_memory_obligation(
        &MemoryObligationContext {
            namespace_id: "namespace-a".into(),
            turn_id: turn,
            execution_id: execution,
            recorded_at: "2026-08-30T00:00:02Z".into(),
        },
        &obligation,
    )
    .unwrap();
    ledger
        .commit(session.clone(), 2, vec![obligation_fact])
        .unwrap();

    let recall_fact = ledger.read_facts(&session, 3, 4, None).unwrap().remove(0);
    let observation = MemoryObservation::new(
        "observation",
        "obligation",
        6,
        "verifier-v1",
        vec![ObservationEvidence::new(
            ObservationEvidenceKind::TestResult,
            DurableFactReference::new(
                session.as_str(),
                4,
                recall_fact.fact_id.as_str(),
                recall_fact.payload.sha256(),
            )
            .unwrap(),
        )],
        ObservationVerdict::Verified,
    )
    .unwrap();
    let lifecycle = MemoryLifecycle::new(
        HypothesisState::Candidate,
        EvidenceTally {
            verified: 0,
            falsified: 0,
            neutral: 0,
        },
        1,
        None,
    )
    .unwrap();
    let planned = plan_memory_observation(
        &MemoryObservationContext {
            namespace_id: "namespace-a".into(),
            recorded_at: "2026-08-30T00:00:03Z".into(),
        },
        &obligation,
        &observation,
        &lifecycle,
    )
    .unwrap();
    assert_eq!(planned.reduction.lifecycle.state(), HypothesisState::Active);
    ledger.commit(session.clone(), 3, planned.facts).unwrap();
    let foreign = MemoryObligation::new(
        "obligation-b",
        "record-b",
        "revision-b",
        obligation.application_fact().clone(),
        "e".repeat(64),
        "f".repeat(64),
        "attribution-v1",
        30,
    )
    .unwrap();
    let foreign_fact = plan_memory_obligation(
        &MemoryObligationContext {
            namespace_id: "namespace-b".into(),
            turn_id: TurnId::try_from("turn").unwrap(),
            execution_id: ExecutionId::try_from("execution").unwrap(),
            recorded_at: "2026-08-30T00:00:04Z".into(),
        },
        &foreign,
    )
    .unwrap();
    ledger
        .commit(session.clone(), 4, vec![foreign_fact])
        .unwrap();
    drop(ledger);

    let restarted = SqliteLedger::open(&path).unwrap();
    let facts = restarted.read_facts(&session, 3, 8, None).unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "memory.recall_recorded",
            "memory.obligation_opened",
            "memory.observation_recorded",
            "memory.lifecycle_transitioned",
            "memory.obligation_opened",
        ]
    );
    assert!(facts[2].turn_id.is_none() && facts[2].execution_id.is_none());
    assert!(facts[3].turn_id.is_none() && facts[3].execution_id.is_none());
    let before_observation = [MemoryPrefix {
        session_id: session.clone(),
        through_position: 5,
    }];
    assert!(reconstruct_memory_hypothesis_projection(
        &restarted,
        &before_observation,
        "namespace-a"
    )
    .unwrap()
    .open_obligation("obligation")
    .is_some());
    let torn_prefix = [MemoryPrefix {
        session_id: session.clone(),
        through_position: 6,
    }];
    assert_eq!(
        reconstruct_memory_hypothesis_projection(&restarted, &torn_prefix, "namespace-a"),
        Err(MemoryErrorCode::CorruptMemoryState),
    );
    let prefixes = [MemoryPrefix {
        session_id: session,
        through_position: 8,
    }];
    let projection_a =
        reconstruct_memory_hypothesis_projection(&restarted, &prefixes, "namespace-a").unwrap();
    assert_eq!(projection_a.recalls().len(), 1);
    assert_eq!(
        projection_a
            .lifecycle("record", "revision")
            .unwrap()
            .state(),
        HypothesisState::Active
    );
    assert!(projection_a.open_obligation("obligation").is_none());
    assert!(projection_a.open_obligation("obligation-b").is_none());
    let projection_b =
        reconstruct_memory_hypothesis_projection(&restarted, &prefixes, "namespace-b").unwrap();
    assert!(projection_b.recalls().is_empty());
    assert!(projection_b.lifecycle("record", "revision").is_none());
    assert_eq!(
        projection_b
            .open_obligation("obligation-b")
            .unwrap()
            .record_id(),
        "record-b"
    );
}

fn runtime_payload(kind: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger/runtime-facts-v1.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    fixture["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["kind"].as_str() == Some(kind))
        .map(|value| value["payload"].clone())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn draft(id: &str, kind: &str, turn: Option<&str>, execution: Option<&str>) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: turn.map(|value| TurnId::try_from(value).unwrap()),
        execution_id: execution.map(|value| ExecutionId::try_from(value).unwrap()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&runtime_payload(kind)).unwrap(),
        recorded_at: "2026-08-30T00:00:00Z".into(),
    }
}

fn initial_facts() -> Vec<FactDraft> {
    vec![
        draft("session-opened", "session.opened", None, None),
        draft("turn-started", "turn.started", Some("turn"), None),
        draft(
            "execution-started",
            "execution.started",
            Some("turn"),
            Some("execution"),
        ),
    ]
}
