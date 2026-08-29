use std::path::PathBuf;

use garive_ledger::{
    CanonicalPayload, CommitDisposition, ExecutionId, FactDraft, FactId, FactKind, LedgerError,
    SessionId, TurnId,
};
use garive_memory::{
    ContentBinding, DurableFactReference, MemoryCommit, MemoryKind, MemoryProposal, MemoryScope,
    MemorySensitivity, MemoryState, MemoryTombstone,
};
use garive_runtime::{
    plan_memory_tombstone, plan_memory_write, MemoryTombstoneContext, MemoryTombstoneReason,
    MemoryWriteContext, MemoryWriteDecision, MemoryWriteRejection, RuntimeCommandError,
    SqliteLedger, SqliteLedgerError,
};
use serde_json::{json, Value};
use tempfile::tempdir;

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
        .unwrap_or_else(|| json!({}))
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
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn initial_facts() -> Vec<FactDraft> {
    vec![
        draft("evidence", "session.opened", None, None),
        draft("turn-fact", "turn.started", Some("turn"), None),
        draft(
            "execution-fact",
            "execution.started",
            Some("turn"),
            Some("execution"),
        ),
    ]
}

fn context(through_position: u64) -> MemoryWriteContext {
    MemoryWriteContext {
        turn_id: TurnId::try_from("turn").unwrap(),
        execution_id: ExecutionId::try_from("execution").unwrap(),
        through_position,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

fn proposal(expected: Option<&str>, content: &str, payload_digest: &str) -> MemoryProposal {
    MemoryProposal::new(
        if expected.is_some() {
            "proposal-2"
        } else {
            "proposal-1"
        },
        "namespace",
        MemoryScope::session("session").unwrap(),
        MemoryKind::Preference,
        ContentBinding::from_inline(content),
        vec![DurableFactReference::new("session", 1, "evidence", payload_digest).unwrap()],
        MemorySensitivity::Ordinary,
        9_000,
        expected.map(str::to_owned),
    )
    .unwrap()
}

fn commit(record: &str, revision: &str, position: u64, prior: Option<&str>) -> MemoryCommit {
    MemoryCommit::new(
        record,
        revision,
        "a".repeat(64),
        position,
        None,
        prior.map(str::to_owned),
    )
    .unwrap()
}

#[test]
fn sqlite_write_batches_are_atomic_replayable_and_restart_safe() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger.commit(session.clone(), 0, initial_facts()).unwrap();
    let evidence_digest = ledger.read_facts(&session, 0, 1, None).unwrap()[0]
        .payload
        .sha256()
        .to_owned();
    let proposal = proposal(None, "dark mode", &evidence_digest);
    let first = plan_memory_write(
        &context(3),
        &MemoryState::default(),
        &proposal,
        MemoryWriteDecision::Commit(commit("record", "revision-1", 5, None)),
    )
    .unwrap();
    let result = ledger
        .commit(session.clone(), 1, first.facts.clone())
        .unwrap();
    assert_eq!(result.positions, vec![4, 5]);
    assert_eq!(
        ledger
            .commit(session.clone(), 1, first.facts.clone())
            .unwrap()
            .disposition,
        CommitDisposition::Replayed
    );
    drop(ledger);

    let mut ledger = SqliteLedger::open(&path).unwrap();
    let facts = ledger.read_facts(&session, 0, 5, None).unwrap();
    assert_eq!(facts[3].kind.as_str(), "memory.proposed");
    assert_eq!(facts[4].kind.as_str(), "memory.committed");

    let changed = plan_memory_write(
        &context(3),
        &MemoryState::default(),
        &proposal,
        MemoryWriteDecision::Reject(MemoryWriteRejection::NamespaceDenied),
    )
    .unwrap();
    assert!(matches!(
        ledger.commit(session.clone(), 2, changed.facts),
        Err(SqliteLedgerError::Domain(LedgerError::IncompleteReplay))
    ));
    assert_eq!(
        ledger
            .session_watermark(&session)
            .unwrap()
            .unwrap()
            .max_position,
        5
    );
}

#[test]
fn supersession_and_tombstone_require_the_exact_active_revision() {
    let evidence = "b".repeat(64);
    let first_proposal = proposal(None, "one", &evidence);
    let first = plan_memory_write(
        &context(3),
        &MemoryState::default(),
        &first_proposal,
        MemoryWriteDecision::Commit(commit("record", "revision-1", 5, None)),
    )
    .unwrap();
    let second_proposal = proposal(Some("revision-1"), "two", &evidence);
    let second = plan_memory_write(
        &context(5),
        &first.next_state,
        &second_proposal,
        MemoryWriteDecision::Commit(commit("record", "revision-2", 7, Some("revision-1"))),
    )
    .unwrap();
    assert_eq!(second.facts.len(), 3);
    assert_eq!(second.facts[2].kind.as_str(), "memory.superseded");
    assert_eq!(
        plan_memory_write(
            &context(5),
            &first.next_state,
            &second_proposal,
            MemoryWriteDecision::Commit(commit("record", "revision-2", 8, Some("revision-1"))),
        )
        .err()
        .unwrap(),
        RuntimeCommandError::InvalidCommand
    );

    let tombstone = plan_memory_tombstone(
        &MemoryTombstoneContext {
            command_id: "forget".into(),
            recorded_at: "2026-08-29T00:00:02Z".into(),
        },
        &second.next_state,
        &MemoryTombstone {
            record_id: "record".into(),
            revision_id: "revision-2".into(),
        },
        MemoryTombstoneReason::UserRequest,
    )
    .unwrap();
    assert!(tombstone.fact.turn_id.is_none() && tombstone.fact.execution_id.is_none());
    assert_eq!(
        plan_memory_tombstone(
            &MemoryTombstoneContext {
                command_id: "stale".into(),
                recorded_at: "2026-08-29T00:00:02Z".into(),
            },
            &tombstone.next_state,
            &MemoryTombstone {
                record_id: "record".into(),
                revision_id: "revision-2".into(),
            },
            MemoryTombstoneReason::Policy,
        )
        .err()
        .unwrap(),
        RuntimeCommandError::CommandConflict
    );
}
