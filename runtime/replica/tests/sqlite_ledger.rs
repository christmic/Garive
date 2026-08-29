use garive_ledger::{
    CanonicalPayload, CommitDisposition, ExecutionId, FactDraft, FactId, FactKind, LedgerError,
    ModelRequestId, SessionId, ToolInvocationId, TurnId,
};
use garive_runtime::{SqliteLedger, SqliteLedgerError};
use serde_json::{json, Value};
use tempfile::tempdir;

fn draft(
    id: &str,
    kind: &str,
    turn: Option<&str>,
    execution: Option<&str>,
    request: Option<&str>,
) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: turn.map(|value| TurnId::try_from(value).unwrap()),
        execution_id: execution.map(|value| ExecutionId::try_from(value).unwrap()),
        model_request_id: request.map(|value| ModelRequestId::try_from(value).unwrap()),
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&runtime_payload(kind)).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn runtime_payload(kind: &str) -> Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

fn initial_facts() -> Vec<FactDraft> {
    vec![
        draft("f1", "session.opened", None, None, None),
        draft("f2", "turn.started", Some("t1"), None, None),
        draft("f3", "execution.started", Some("t1"), Some("e1"), None),
    ]
}

fn tool_draft(id: &str, kind: &str, tool: &str) -> FactDraft {
    let mut value = draft(id, kind, Some("t1"), Some("e1"), None);
    value.tool_invocation_id = Some(ToolInvocationId::try_from(tool).unwrap());
    value
}

#[test]
fn file_database_reopens_with_durable_order_and_policy() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        let foreign_keys: u32 = ledger
            .connection_for_test()
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        let synchronous: u32 = ledger
            .connection_for_test()
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        let busy_timeout: u32 = ledger
            .connection_for_test()
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        let journal_mode: String = ledger
            .connection_for_test()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        assert_eq!(synchronous, 2);
        assert_eq!(busy_timeout, 5_000);
        assert_eq!(journal_mode, "wal");
        let result = ledger.commit(session.clone(), 0, initial_facts()).unwrap();
        assert_eq!(result.positions, vec![1, 2, 3]);
        assert_eq!(result.session_version, 1);
    }

    let ledger = SqliteLedger::open(&path).unwrap();
    assert_eq!(ledger.session_version(&session).unwrap(), Some(1));
    let facts = ledger.read_facts(&session, 0, 3, None).unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        ["session.opened", "turn.started", "execution.started"]
    );
}

#[test]
fn migrations_advance_v1_and_refuse_unknown_future_schema() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("migration.sqlite3");
    {
        let ledger = SqliteLedger::open(&path).unwrap();
        ledger
            .connection_for_test()
            .execute_batch(
                "DROP TABLE execution_leases; \
                 DROP TABLE schedule_leases; \
                 DELETE FROM schema_migrations WHERE version >= 2;",
            )
            .unwrap();
    }
    {
        let migrated = SqliteLedger::open(&path).unwrap();
        let version: u32 = migrated
            .connection_for_test()
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 3);
        let leases: u32 = migrated
            .connection_for_test()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='execution_leases'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(leases, 1);
        let schedule_leases: u32 = migrated
            .connection_for_test()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='schedule_leases'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(schedule_leases, 1);
        migrated
            .connection_for_test()
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (4, ?1)",
                ["2026-08-29T00:00:00Z"],
            )
            .unwrap();
    }
    assert!(matches!(
        SqliteLedger::open(path),
        Err(SqliteLedgerError::UnsupportedSchema(4))
    ));
}

#[test]
fn conflict_replay_and_invalid_batch_leave_no_partial_facts() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("atomic.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    let first = initial_facts();
    ledger.commit(session.clone(), 0, first.clone()).unwrap();

    let replay = ledger.commit(session.clone(), 0, first).unwrap();
    assert_eq!(replay.disposition, CommitDisposition::Replayed);
    let mut collision = draft("f1", "session.opened", None, None, None);
    collision.payload = CanonicalPayload::from_value(&json!({"changed": true})).unwrap();
    let collision = ledger
        .commit(session.clone(), 1, vec![collision])
        .unwrap_err();
    assert!(matches!(
        collision,
        SqliteLedgerError::Domain(LedgerError::IdempotencyCollision)
    ));
    let conflict = ledger
        .commit(
            session.clone(),
            0,
            vec![draft("f4", "privacy.redacted", None, None, None)],
        )
        .unwrap_err();
    assert!(matches!(
        conflict,
        SqliteLedgerError::Domain(LedgerError::ConcurrentModification)
    ));

    let invalid = ledger
        .commit(
            session.clone(),
            1,
            vec![
                draft("f5", "execution.completed", Some("t1"), Some("e1"), None),
                draft("f6", "execution.failed", Some("t1"), Some("e1"), None),
            ],
        )
        .unwrap_err();
    assert!(matches!(
        invalid,
        SqliteLedgerError::Domain(LedgerError::InvalidTransition)
    ));
    drop(ledger);

    let reopened = SqliteLedger::open(&path).unwrap();
    assert_eq!(reopened.session_version(&session).unwrap(), Some(1));
    assert_eq!(reopened.read_facts(&session, 0, 3, None).unwrap().len(), 3);
}

#[test]
fn corrupted_canonical_digest_fails_closed_after_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("corrupt.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        ledger.commit(session.clone(), 0, initial_facts()).unwrap();
        ledger
            .connection_for_test()
            .execute(
                "UPDATE ledger_facts SET payload_sha256 = '0000000000000000000000000000000000000000000000000000000000000000' \
                 WHERE fact_id = 'f2'",
                [],
            )
            .unwrap();
    }
    let reopened = SqliteLedger::open(&path).unwrap();
    assert!(matches!(
        reopened.session_version(&session),
        Err(SqliteLedgerError::CorruptLedger(LedgerError::Corruption(_)))
    ));
}

#[test]
fn started_model_is_uncertain_after_connection_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("recovery.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        let mut facts = initial_facts();
        facts.extend([
            draft("f4", "model.prepared", Some("t1"), Some("e1"), Some("r1")),
            draft("f5", "model.started", Some("t1"), Some("e1"), Some("r1")),
        ]);
        ledger.commit(session.clone(), 0, facts).unwrap();
    }
    let reopened = SqliteLedger::open(&path).unwrap();
    assert_eq!(
        reopened
            .list_uncertain_model_requests(&session)
            .unwrap()
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        ["r1"]
    );
}

#[test]
fn started_tool_is_uncertain_after_connection_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("effect-recovery.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        let mut facts = initial_facts();
        facts.extend([
            tool_draft("f4", "effect.prepared", "tool1"),
            tool_draft("f5", "effect.started", "tool1"),
        ]);
        ledger.commit(session.clone(), 0, facts).unwrap();
    }
    let reopened = SqliteLedger::open(&path).unwrap();
    assert_eq!(
        reopened
            .list_uncertain_tool_invocations(&session)
            .unwrap()
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        ["tool1"]
    );
}
