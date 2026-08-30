use std::{collections::BTreeMap, path::PathBuf};

use garive_core::{derive_context_with_memory, ContextPurpose, ContextRequest};

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, ModelRequestId, SessionId, TurnId,
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

    ledger
        .commit(
            session.clone(),
            2,
            vec![
                model_draft("model-prepared", "model.prepared"),
                model_draft("model-started", "model.started"),
                model_draft("model-completed", "model.completed"),
            ],
        )
        .unwrap();
    let application = ledger.read_facts(&session, 6, 7, None).unwrap().remove(0);
    let obligation = MemoryObligation::new(
        "obligation",
        "record",
        "revision",
        DurableFactReference::new(
            session.as_str(),
            recall_fact.position,
            recall_fact.fact_id.as_str(),
            recall_fact.payload.sha256(),
        )
        .unwrap(),
        "selection",
        DurableFactReference::new(
            session.as_str(),
            application.position,
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
    let unselected = MemoryObligation::new(
        "unselected",
        "missing-record",
        "missing-revision",
        obligation.recall_fact().clone(),
        obligation.selection_id(),
        obligation.application_fact().clone(),
        "c".repeat(64),
        "d".repeat(64),
        "attribution-v1",
        20,
    )
    .unwrap();
    assert!(plan_memory_obligation(
        &MemoryObligationContext {
            namespace_id: "namespace-a".into(),
            turn_id: turn.clone(),
            execution_id: execution.clone(),
            recorded_at: "2026-08-30T00:00:02Z".into(),
        },
        &recall_fact,
        &application,
        &unselected,
    )
    .is_err());
    let obligation_fact = plan_memory_obligation(
        &MemoryObligationContext {
            namespace_id: "namespace-a".into(),
            turn_id: turn,
            execution_id: execution,
            recorded_at: "2026-08-30T00:00:02Z".into(),
        },
        &recall_fact,
        &application,
        &obligation,
    )
    .unwrap();
    let mut forged_obligation = obligation_fact.clone();
    forged_obligation.fact_id = FactId::try_from("fact-forged-obligation").unwrap();
    let mut forged_payload: Value =
        serde_json::from_str(forged_obligation.payload.as_json()).unwrap();
    forged_payload["obligation_id"] = Value::String("forged-obligation".into());
    forged_payload["selection_id"] = Value::String("wrong-selection".into());
    forged_obligation.payload = CanonicalPayload::from_value(&forged_payload).unwrap();
    ledger
        .commit(session.clone(), 3, vec![obligation_fact])
        .unwrap();

    let observation = MemoryObservation::new(
        "observation",
        "obligation",
        9,
        "verifier-v1",
        vec![ObservationEvidence::new(
            ObservationEvidenceKind::TestResult,
            DurableFactReference::new(
                session.as_str(),
                application.position,
                application.fact_id.as_str(),
                application.payload.sha256(),
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
        7,
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
    ledger.commit(session.clone(), 4, planned.facts).unwrap();
    drop(ledger);

    let restarted = SqliteLedger::open(&path).unwrap();
    let facts = restarted.read_facts(&session, 3, 10, None).unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "memory.recall_recorded",
            "model.prepared",
            "model.started",
            "model.completed",
            "memory.obligation_opened",
            "memory.observation_recorded",
            "memory.lifecycle_transitioned",
        ]
    );
    assert!(facts[5].turn_id.is_none() && facts[5].execution_id.is_none());
    assert!(facts[6].turn_id.is_none() && facts[6].execution_id.is_none());
    let before_observation = [MemoryPrefix {
        session_id: session.clone(),
        through_position: 8,
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
        through_position: 9,
    }];
    assert_eq!(
        reconstruct_memory_hypothesis_projection(&restarted, &torn_prefix, "namespace-a"),
        Err(MemoryErrorCode::CorruptMemoryState),
    );
    let prefixes = [MemoryPrefix {
        session_id: session.clone(),
        through_position: 10,
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
    assert!(projection_b.open_obligation("obligation").is_none());
    ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 5, vec![forged_obligation])
        .unwrap();
    let corrupt_prefix = [MemoryPrefix {
        session_id: session,
        through_position: 11,
    }];
    assert_eq!(
        reconstruct_memory_hypothesis_projection(&ledger, &corrupt_prefix, "namespace-a"),
        Err(MemoryErrorCode::CorruptMemoryState),
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

fn model_draft(id: &str, kind: &str) -> FactDraft {
    let mut fact = draft(id, kind, Some("turn"), Some("execution"));
    fact.model_request_id = Some(ModelRequestId::try_from("request").unwrap());
    fact
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
