use garive_runtime::{SqliteLedger, PERSISTENT_MONOTONIC_CLOCK_REVISION};
use tempfile::tempdir;

#[test]
fn persistent_clock_survives_process_and_boot_boundaries_without_wall_time() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("persistent-clock.sqlite3");
    let mut first = SqliteLedger::open(&path).unwrap();
    let initial = first.reserve_monotonic_lease("boot-a", 10_000, 50).unwrap();
    assert_eq!(initial.clock_revision, PERSISTENT_MONOTONIC_CLOCK_REVISION);
    assert_eq!(initial.now_ms, 1);
    let advanced = first.reserve_monotonic_lease("boot-a", 10_010, 50).unwrap();
    assert_eq!(advanced.now_ms, 11);
    drop(first);

    let mut restarted = SqliteLedger::open(&path).unwrap();
    assert_eq!(
        restarted
            .reserve_monotonic_lease("boot-a", 10_020, 50)
            .unwrap()
            .now_ms,
        21
    );
    let next_boot = restarted.reserve_monotonic_lease("boot-b", 5, 50).unwrap();
    assert_eq!(
        next_boot.clock_revision,
        PERSISTENT_MONOTONIC_CLOCK_REVISION
    );
    assert_eq!(next_boot.now_ms, 72);
    assert!(next_boot.now_ms > advanced.now_ms + 50);
    assert_eq!(
        restarted
            .reserve_monotonic_lease("boot-b", 15, 50)
            .unwrap()
            .now_ms,
        82
    );
}

#[test]
fn persistent_clock_rejects_invalid_or_regressed_boot_inputs() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("persistent-clock-invalid.sqlite3");
    let mut ledger = SqliteLedger::open(&path).unwrap();
    assert!(ledger.reserve_monotonic_lease("", 10, 5).is_err());
    assert!(ledger.reserve_monotonic_lease("boot-a", 10, 0).is_err());
    ledger.reserve_monotonic_lease("boot-a", 10, 5).unwrap();
    assert!(ledger.reserve_monotonic_lease("boot-a", 9, 5).is_err());
}
