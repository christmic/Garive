#[path = "support/memory_control_values.rs"]
mod support;

use garive_memory::{MemoryDocumentLimits, MemorySnapshotLimits};
use garive_runtime::{
    export_memory_snapshot, MemoryControlRuntimeError, MemoryExportCommand, MemoryExportTarget,
    SqliteLedger,
};
use support::*;
use tempfile::tempdir;

#[test]
fn export_is_exact_replayable_and_contains_no_path_in_receipt() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("memory.sqlite");
    let destination = directory.path().join("snapshot");
    let originals = originals();
    let grant = grant("namespace-export", scope_set());
    let command = command("one", "namespace-export");
    let target = MemoryExportTarget::authorized(&destination, "a".repeat(64)).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    ledger
        .initialize_memory_control_namespace(&grant, "namespace-export", 11, &originals)
        .unwrap();

    let receipt = export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()).unwrap();
    assert_eq!(receipt.through_repository_revision, 11);
    assert_eq!(receipt.entry_count, 3);
    assert!(destination.join("manifest.json").is_file());
    assert_eq!(
        std::fs::read_dir(destination.join("entries"))
            .unwrap()
            .count(),
        3
    );
    let public_json = serde_jcs::to_string(&receipt).unwrap();
    assert!(!public_json.contains(destination.to_str().unwrap()));
    assert!(!public_json.contains("old a"));

    let replay = export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()).unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(
        scalar(
            ledger.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal WHERE event_kind='export'"
        ),
        1
    );
    ledger
        .connection_for_test()
        .execute(
            "UPDATE memory_namespaces SET repository_revision=?1 WHERE namespace_id='namespace-export'",
            [12_u64.to_be_bytes()],
        )
        .unwrap();
    assert_eq!(
        export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()).unwrap(),
        receipt
    );
    std::fs::write(destination.join("manifest.json"), b"{}").unwrap();
    assert_eq!(
        export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()),
        Err(MemoryControlRuntimeError::ExportTargetInvalid)
    );
}

#[test]
fn rename_before_database_failure_is_repaired_on_retry() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("memory.sqlite");
    let destination = directory.path().join("recovered-snapshot");
    let originals = originals();
    let grant = grant("namespace-recovery", scope_set());
    let command = command("recovery", "namespace-recovery");
    let target = MemoryExportTarget::authorized(&destination, "b".repeat(64)).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    ledger
        .initialize_memory_control_namespace(&grant, "namespace-recovery", 5, &originals)
        .unwrap();
    ledger
        .connection_for_test()
        .execute_batch(
            "CREATE TRIGGER fail_export BEFORE INSERT ON memory_control_journal \
             WHEN NEW.event_kind='export' BEGIN SELECT RAISE(ABORT, 'forced'); END;",
        )
        .unwrap();

    assert_eq!(
        export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()),
        Err(MemoryControlRuntimeError::PersistenceFailed)
    );
    assert!(destination.is_dir());
    assert_eq!(
        scalar(
            ledger.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal"
        ),
        0
    );
    assert_eq!(hidden_journals(directory.path()), 1);

    ledger
        .connection_for_test()
        .execute_batch("DROP TRIGGER fail_export;")
        .unwrap();
    let receipt = export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()).unwrap();
    assert_eq!(receipt.through_repository_revision, 5);
    assert_eq!(hidden_journals(directory.path()), 0);
    assert_eq!(
        scalar(
            ledger.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal WHERE event_kind='export'"
        ),
        1
    );
}

#[test]
fn occupied_or_tampered_destination_fails_closed() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("memory.sqlite");
    let destination = directory.path().join("occupied");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("foreign"), b"foreign").unwrap();
    let originals = originals();
    let grant = grant("namespace-target", scope_set());
    let command = command("target", "namespace-target");
    let target = MemoryExportTarget::authorized(&destination, "c".repeat(64)).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    ledger
        .initialize_memory_control_namespace(&grant, "namespace-target", 2, &originals)
        .unwrap();
    assert_eq!(
        export_memory_snapshot(&mut ledger, &grant, &command, &target, limits()),
        Err(MemoryControlRuntimeError::ExportTargetInvalid)
    );
    assert_eq!(
        scalar(
            ledger.connection_for_test(),
            "SELECT COUNT(*) FROM memory_control_journal"
        ),
        0
    );
}

fn command(suffix: &str, namespace: &str) -> MemoryExportCommand {
    MemoryExportCommand::new(
        format!("command-{suffix}"),
        format!("receipt-{suffix}"),
        format!("event-{suffix}"),
        format!("export-{suffix}"),
        namespace,
        "2026-08-30T00:00:00Z",
    )
    .unwrap()
}

fn limits() -> MemorySnapshotLimits {
    MemorySnapshotLimits::new(
        16,
        64 * 1024,
        MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
    )
    .unwrap()
}

fn hidden_journals(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".journal"))
        .count()
}
