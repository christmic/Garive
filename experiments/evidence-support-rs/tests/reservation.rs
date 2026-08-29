use std::fs;

use garive_experiment_evidence::reserve_evidence_file;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn abandoned_reservation_is_removed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("evidence.json");

    drop(reserve_evidence_file(path.clone()).unwrap());

    assert!(!path.exists());
}

#[test]
fn commit_is_synchronized_pretty_json_and_never_overwrites() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("evidence.json");
    let mut reservation = reserve_evidence_file(path.clone()).unwrap();

    reservation.commit_json(&json!({"count": 2})).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "{\n  \"count\": 2\n}\n");
    assert!(reserve_evidence_file(path).is_err());
}
