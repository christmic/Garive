#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/persistence/values.rs"]
mod values;
pub(crate) use values::{PendingCommand, PendingKind, Preferences, PromptHistoryEntry};
#[path = "../src/persistence/store.rs"]
mod store;

use std::{fs, os::unix::fs::PermissionsExt};

use serde_json::json;
use store::{StateError, StateStore};

#[test]
fn preferences_round_trip_atomically_with_private_permissions() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let store = StateStore::open(Some(root.clone()), false).unwrap();
    let mut preferences = Preferences {
        theme: Theme::Dark,
        ..Preferences::default()
    };
    store.save_preferences(&mut preferences).unwrap();
    assert_eq!(preferences.revision, 1);
    assert_eq!(store.load_preferences().unwrap(), preferences);
    assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o077, 0);
    assert_eq!(
        fs::metadata(root.join("preferences.v1.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("tmp-")));
}

#[test]
fn pending_digest_is_exact_and_conflicts_are_refused() {
    let temporary = tempfile::tempdir().unwrap();
    let store = StateStore::open(Some(temporary.path().join("state")), false).unwrap();
    let pending = command("one", "hello");
    store.save_pending(&pending).unwrap();
    store.save_pending(&pending).unwrap();
    let conflict = command("two", "different");
    assert_eq!(store.save_pending(&conflict), Err(StateError::Conflict));
    store.remove_pending(Some("session-1")).unwrap();
}

#[test]
fn hostile_permissions_and_invalid_preference_shape_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        StateStore::open(Some(root), false).unwrap_err(),
        StateError::UnsafePermissions
    );
}

#[test]
fn preference_writes_compare_and_swap_and_corruption_is_quarantined() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let store = StateStore::open(Some(root.clone()), false).unwrap();
    let mut first = store.load_preferences().unwrap();
    let mut stale = first.clone();
    first.theme = Theme::Dark;
    store.save_preferences(&mut first).unwrap();
    stale.theme = Theme::Light;
    assert_eq!(
        store.save_preferences(&mut stale),
        Err(StateError::Conflict)
    );
    assert_eq!(store.load_preferences().unwrap().theme, Theme::Dark);

    fs::write(root.join("preferences.v1.json"), b"{broken}").unwrap();
    assert_eq!(store.load_preferences().unwrap(), Preferences::default());
    assert_eq!(fs::read_dir(root.join("quarantine")).unwrap().count(), 1);
}

#[test]
fn history_compacts_duplicates_ignores_torn_tail_and_quarantines_corruption() {
    use std::io::Write;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let store = StateStore::open(Some(root.clone()), false).unwrap();
    let entry = PromptHistoryEntry {
        schema_version: 1,
        entry_id: "entry-1".into(),
        session_id: "session-1".into(),
        submitted_text: "hello".into(),
        submitted_at: "2026-08-30T00:00:00Z".into(),
    };
    store.append_history(&entry).unwrap();
    store.append_history(&entry).unwrap();
    assert_eq!(store.load_history().unwrap().len(), 1);
    let path = root.join("prompt-history.v1.jsonl");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{torn")
        .unwrap();
    assert_eq!(store.load_history().unwrap().len(), 1);
    fs::write(&path, b"{corrupt}\n").unwrap();
    assert_eq!(store.load_history(), Err(StateError::InvalidData));
    assert_eq!(fs::read_dir(root.join("quarantine")).unwrap().count(), 1);
}

fn command(id: &str, text: &str) -> PendingCommand {
    PendingCommand {
        schema_version: 1,
        command_id: id.into(),
        kind: PendingKind::StartTurn,
        session_id: Some("session-1".into()),
        turn_id: None,
        suspension_id: None,
        expected_session_version: None,
        requested_through_position: None,
        request_payload: json!({"text": text}),
        request_digest: String::new(),
        created_at: "2026-08-30T00:00:00Z".into(),
    }
    .seal()
    .unwrap()
}
