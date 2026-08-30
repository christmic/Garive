#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/persistence/values.rs"]
mod values;
pub(crate) use values::{PendingCommand, PendingKind, Preferences, PromptHistoryEntry};
#[path = "../src/persistence/store.rs"]
mod store;

use std::fs;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use serde_json::json;
use store::{DiagnosticEvent, StateError, StateStore};

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
    assert_private(&root);
    assert_private(&root.join("preferences.v1.json"));
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("tmp-")));
}

#[test]
fn startup_removes_only_grammatically_owned_abandoned_temps() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let _ = StateStore::open(Some(root.clone()), false).unwrap();
    let suffix = uuid::Uuid::new_v4();
    let preference_temp = root.join(format!("preferences.v1.tmp-{suffix}"));
    let pending_temp = root
        .join("pending")
        .join(format!("{}.v1.tmp-{suffix}", "a".repeat(64)));
    let foreign = root.join(format!("customer-data.tmp-{suffix}"));
    for path in [&preference_temp, &pending_temp, &foreign] {
        fs::write(path, b"stale").unwrap();
        make_private_fixture(path);
    }

    let _ = StateStore::open(Some(root), false).unwrap();

    assert!(!preference_temp.exists());
    assert!(!pending_temp.exists());
    assert!(foreign.exists());
}

#[test]
fn diagnostics_are_content_free_private_and_bounded_to_five_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let store = StateStore::open(Some(root.clone()), false).unwrap();
    let canary = "secret-prompt-host-path-canary";
    let mut preferences = Preferences {
        selected_session_id: Some(canary.into()),
        ..Preferences::default()
    };
    store.save_preferences(&mut preferences).unwrap();
    store.record_diagnostic(DiagnosticEvent::Started).unwrap();
    store
        .record_diagnostic(DiagnosticEvent::HostFailure {
            safe_code: "host_failure",
        })
        .unwrap();
    let directory = root.join("diagnostics");
    let active = directory.join("garive-tui.log");
    let first = fs::read_to_string(&active).unwrap();
    assert!(first.contains("tui_started"));
    assert!(first.contains("host_failure"));
    assert!(!first.contains(canary));
    assert_private(&active);

    for _ in 0..6 {
        fs::write(&active, vec![b'x'; 1_048_576]).unwrap();
        store
            .record_diagnostic(DiagnosticEvent::RetryQueued)
            .unwrap();
    }
    let files = fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("garive-tui.log")
        })
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 5);
    assert!(files
        .iter()
        .all(|entry| fs::metadata(entry.path()).unwrap().len() <= 1_048_576));
}

#[test]
fn pending_digest_is_exact_and_conflicts_are_refused() {
    let temporary = tempfile::tempdir().unwrap();
    let store = StateStore::open(Some(temporary.path().join("state")), false).unwrap();
    let pending = command("one", "hello");
    store.save_pending(&pending).unwrap();
    store.save_pending(&pending).unwrap();
    store
        .save_pending(&command_for("session-2", "two", "independent"))
        .unwrap();
    let (loaded, quarantined) = store.load_pending().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(quarantined, 0);
    let conflict = command("two", "different");
    assert_eq!(store.save_pending(&conflict), Err(StateError::Conflict));
    store.remove_pending(Some("session-1")).unwrap();
}

#[test]
fn corrupt_pending_is_quarantined_without_hiding_valid_sessions() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    let store = StateStore::open(Some(root.clone()), false).unwrap();
    store.save_pending(&command("one", "hello")).unwrap();
    store
        .save_pending(&command_for("corrupt-session", "bad", "replace me"))
        .unwrap();
    let corrupt = fs::read_dir(root.join("pending"))
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| {
            fs::read_to_string(entry.path()).is_ok_and(|value| value.contains("corrupt-session"))
        })
        .unwrap()
        .path();
    fs::write(&corrupt, b"{broken}").unwrap();

    let (loaded, quarantined) = store.load_pending().unwrap();
    assert_eq!(loaded, vec![command("one", "hello")]);
    assert_eq!(quarantined, 1);
    assert_eq!(fs::read_dir(root.join("quarantine")).unwrap().count(), 1);
}

#[test]
fn hostile_permissions_and_invalid_preference_shape_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("state");
    fs::create_dir(&root).unwrap();
    make_hostile_fixture(&root);
    assert_eq!(
        StateStore::open(Some(root), false).unwrap_err(),
        StateError::UnsafePermissions
    );
}

#[cfg(unix)]
fn assert_private(path: &std::path::Path) {
    assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
}

#[cfg(windows)]
fn assert_private(path: &std::path::Path) {
    assert!(path.exists());
}

#[cfg(unix)]
fn make_private_fixture(path: &std::path::Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(windows)]
fn make_private_fixture(_path: &std::path::Path) {}

#[cfg(unix)]
fn make_hostile_fixture(path: &std::path::Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(windows)]
fn make_hostile_fixture(_path: &std::path::Path) {}

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
fn preference_conflicts_merge_independent_fields_and_reject_same_field_edits() {
    let temporary = tempfile::tempdir().unwrap();
    let store = StateStore::open(Some(temporary.path().join("state")), false).unwrap();
    let mut theme_base = store.load_preferences().unwrap();
    let mut mouse_base = theme_base.clone();
    let mut theme = theme_base.clone();
    let mut mouse = mouse_base.clone();
    theme.theme = Theme::Dark;
    mouse.mouse = MouseMode::On;
    store
        .save_preferences_merged(&mut theme, &mut theme_base)
        .unwrap();
    store
        .save_preferences_merged(&mut mouse, &mut mouse_base)
        .unwrap();
    let merged = store.load_preferences().unwrap();
    assert_eq!(merged.theme, Theme::Dark);
    assert_eq!(merged.mouse, MouseMode::On);

    let mut first_base = merged.clone();
    let mut stale_base = merged.clone();
    let mut first = merged.clone();
    let mut stale = merged;
    first.theme = Theme::Light;
    stale.theme = Theme::Mono;
    store
        .save_preferences_merged(&mut first, &mut first_base)
        .unwrap();
    assert_eq!(
        store.save_preferences_merged(&mut stale, &mut stale_base),
        Err(StateError::Conflict)
    );
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
    command_for("session-1", id, text)
}

fn command_for(session: &str, id: &str, text: &str) -> PendingCommand {
    PendingCommand {
        schema_version: 1,
        command_id: id.into(),
        kind: PendingKind::StartTurn,
        session_id: Some(session.into()),
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
