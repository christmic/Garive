#[path = "support/memory_control_values.rs"]
mod support;

use garive_memory::{prepare_memory_import, MemoryDocumentLimits, MemoryIdentityAllocation};
use garive_runtime::{MemoryControlRuntimeError, MemoryImportCommand, SqliteLedger};
use support::*;
use tempfile::tempdir;

#[test]
fn atomic_import_replays_after_restart_and_erases_projection_content() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.sqlite");
    let originals = originals();
    let current = originals.iter().map(current).collect::<Vec<_>>();
    let edited = vec![
        document("mem-a", "rev-a", "user_declared", "active", false, "new a"),
        document(
            "mem-b",
            "rev-b",
            "user_declared",
            "archived",
            false,
            "old b",
        ),
        document("mem-c", "rev-c", "user_declared", "active", true, "old c"),
        new_document("draft-d", "new d"),
    ];
    let plan = prepare_memory_import(
        "export-1",
        "namespace-1",
        7,
        EMPTY_DIGEST,
        7,
        &edited,
        &current,
        &scope_set(),
        &[
            MemoryIdentityAllocation::Supersede {
                record_id: "mem-a".into(),
                revision_id: "rev-a2".into(),
            },
            MemoryIdentityAllocation::Add {
                draft_token: "draft-d".into(),
                record_id: "mem-d".into(),
                revision_id: "rev-d".into(),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        (
            plan.add_count,
            plan.supersede_count,
            plan.archive_count,
            plan.erase_count
        ),
        (1, 1, 1, 1)
    );
    let command =
        MemoryImportCommand::new("command-1", "receipt-1", "event-1", plan, edited, 128).unwrap();
    let grant = grant("namespace-1", scope_set());
    let receipt = {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        ledger
            .initialize_memory_control_namespace(&grant, "namespace-1", 7, &originals)
            .unwrap();
        let receipt = ledger.commit_memory_import(&grant, &command).unwrap();
        assert!(receipt.changed);
        assert_eq!(receipt.previous_repository_revision, 7);
        assert_eq!(receipt.committed_repository_revision, 8);
        let connection = ledger.connection_for_test();
        assert_eq!(
            scalar(connection, "SELECT COUNT(*) FROM memory_control_journal"),
            1
        );
        assert_eq!(
            scalar(connection, "SELECT COUNT(*) FROM memory_control_current"),
            4
        );
        assert_eq!(
            scalar(
                connection,
                "SELECT COUNT(*) FROM memory_control_revisions WHERE document_markdown IS NULL"
            ),
            1
        );
        let erased: (String, Option<String>) = connection
            .query_row(
                "SELECT lifecycle,document_markdown FROM memory_control_current WHERE record_id='mem-c'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(erased, ("erased".into(), None));
        let projection = ledger
            .read_memory_control_projection(
                &grant,
                "namespace-1",
                MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
            )
            .unwrap();
        assert_eq!(projection.repository_revision, 8);
        assert_eq!(projection.documents.len(), 3);
        assert!(projection.documents.iter().all(|document| {
            document.record_ref().record_id() != Some("mem-c")
                && !document.content().contains("old c")
        }));
        receipt
    };
    let mut reopened = SqliteLedger::open(&path).unwrap();
    assert_eq!(
        reopened.commit_memory_import(&grant, &command).unwrap(),
        receipt
    );
    assert_eq!(
        scalar(
            reopened.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal"
        ),
        1
    );
}

#[test]
fn denied_and_stale_commands_roll_back_without_partial_rows() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.sqlite");
    let originals = originals();
    let current = originals.iter().map(current).collect::<Vec<_>>();
    let edited = vec![
        document("mem-a", "rev-a", "user_declared", "active", false, "new a"),
        document(
            "mem-b",
            "rev-b",
            "user_declared",
            "archived",
            false,
            "old b",
        ),
        originals[2].clone(),
    ];
    let plan = prepare_memory_import(
        "export-2",
        "namespace-2",
        4,
        EMPTY_DIGEST,
        4,
        &edited,
        &current,
        &scope_set(),
        &[MemoryIdentityAllocation::Supersede {
            record_id: "mem-a".into(),
            revision_id: "rev-a2".into(),
        }],
    )
    .unwrap();
    let denied_command = MemoryImportCommand::new(
        "command-denied",
        "receipt-denied",
        "event-denied",
        plan.clone(),
        edited.clone(),
        128,
    )
    .unwrap();
    let allowed = grant("namespace-2", scope_set());
    let denied = grant("namespace-2", Vec::new());
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .initialize_memory_control_namespace(&allowed, "namespace-2", 4, &originals)
        .unwrap();
    assert_eq!(
        ledger.commit_memory_import(&denied, &denied_command),
        Err(MemoryControlRuntimeError::Unauthorized)
    );
    assert_eq!(
        scalar(
            ledger.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal"
        ),
        0
    );
    assert_eq!(repository_revision(&ledger), 4);

    let first = MemoryImportCommand::new(
        "command-first",
        "receipt-first",
        "event-first",
        plan.clone(),
        edited.clone(),
        128,
    )
    .unwrap();
    ledger.commit_memory_import(&allowed, &first).unwrap();
    let stale = MemoryImportCommand::new(
        "command-stale",
        "receipt-stale",
        "event-stale",
        plan,
        edited,
        128,
    )
    .unwrap();
    assert_eq!(
        ledger.commit_memory_import(&allowed, &stale),
        Err(MemoryControlRuntimeError::StaleSnapshot)
    );
    assert_eq!(repository_revision(&ledger), 5);
    assert_eq!(
        scalar(
            ledger.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal"
        ),
        1
    );
}

#[test]
fn no_op_is_audited_without_advancing_and_command_rebinding_conflicts() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.sqlite");
    let originals = originals();
    let current = originals.iter().map(current).collect::<Vec<_>>();
    let plan = prepare_memory_import(
        "export-noop",
        "namespace-noop",
        9,
        EMPTY_DIGEST,
        9,
        &originals,
        &current,
        &scope_set(),
        &[],
    )
    .unwrap();
    let command = MemoryImportCommand::new(
        "command-noop",
        "receipt-noop",
        "event-noop",
        plan,
        originals.clone(),
        128,
    )
    .unwrap();
    let grant = grant("namespace-noop", scope_set());
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .initialize_memory_control_namespace(&grant, "namespace-noop", 9, &originals)
        .unwrap();
    let receipt = ledger.commit_memory_import(&grant, &command).unwrap();
    assert!(!receipt.changed);
    assert_eq!(receipt.previous_repository_revision, 9);
    assert_eq!(receipt.committed_repository_revision, 9);

    let changed_documents = vec![
        document(
            "mem-a",
            "rev-a",
            "user_declared",
            "active",
            false,
            "changed",
        ),
        originals[1].clone(),
        originals[2].clone(),
    ];
    let changed_plan = prepare_memory_import(
        "export-noop",
        "namespace-noop",
        9,
        EMPTY_DIGEST,
        9,
        &changed_documents,
        &current,
        &scope_set(),
        &[MemoryIdentityAllocation::Supersede {
            record_id: "mem-a".into(),
            revision_id: "rev-a2".into(),
        }],
    )
    .unwrap();
    let rebound = MemoryImportCommand::new(
        "command-noop",
        "receipt-other",
        "event-other",
        changed_plan,
        changed_documents,
        128,
    )
    .unwrap();
    assert_eq!(
        ledger.commit_memory_import(&grant, &rebound),
        Err(MemoryControlRuntimeError::CommandConflict)
    );
}

#[test]
fn replay_detects_corrupt_durable_event() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.sqlite");
    let originals = originals();
    let current = originals.iter().map(current).collect::<Vec<_>>();
    let plan = prepare_memory_import(
        "export-corrupt",
        "namespace-corrupt",
        3,
        EMPTY_DIGEST,
        3,
        &originals,
        &current,
        &scope_set(),
        &[],
    )
    .unwrap();
    let command = MemoryImportCommand::new(
        "command-corrupt",
        "receipt-corrupt",
        "event-corrupt",
        plan,
        originals.clone(),
        128,
    )
    .unwrap();
    let grant = grant("namespace-corrupt", scope_set());
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .initialize_memory_control_namespace(&grant, "namespace-corrupt", 3, &originals)
        .unwrap();
    ledger.commit_memory_import(&grant, &command).unwrap();
    ledger
        .connection_for_test()
        .execute(
            "UPDATE memory_control_journal SET event_json='{}' WHERE command_id='command-corrupt'",
            [],
        )
        .unwrap();
    assert_eq!(
        ledger.commit_memory_import(&grant, &command),
        Err(MemoryControlRuntimeError::PersistenceFailed)
    );
}
