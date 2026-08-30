use std::{
    collections::BTreeMap,
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use garive_desktop::{
    DesktopWorkspaceBookmarkStore, DesktopWorkspaceError, DesktopWorkspaceService,
    DESKTOP_WORKSPACE_MANIFEST_FILE,
};

#[derive(Default)]
struct MemoryBookmarkStore(Mutex<BTreeMap<String, Vec<u8>>>, AtomicBool);

impl DesktopWorkspaceBookmarkStore for MemoryBookmarkStore {
    fn store(&self, bookmark_ref: &str, bytes: &[u8]) -> Result<(), DesktopWorkspaceError> {
        self.0
            .lock()
            .unwrap()
            .insert(bookmark_ref.into(), bytes.to_vec());
        Ok(())
    }

    fn load(&self, bookmark_ref: &str) -> Result<Vec<u8>, DesktopWorkspaceError> {
        self.0
            .lock()
            .unwrap()
            .get(bookmark_ref)
            .cloned()
            .ok_or(DesktopWorkspaceError::Unavailable)
    }

    fn delete(&self, bookmark_ref: &str) -> Result<(), DesktopWorkspaceError> {
        if self.1.load(Ordering::SeqCst) {
            return Err(DesktopWorkspaceError::Unavailable);
        }
        self.0.lock().unwrap().remove(bookmark_ref);
        Ok(())
    }
}

#[test]
fn selected_directory_becomes_an_owner_bound_path_free_capability() {
    let directory = tempfile::tempdir().unwrap();
    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    assert_eq!(grant.schema_version, 1);
    assert_eq!(grant.access, "enumerate");
    assert_eq!(grant.state, "active");
    assert!(!grant
        .workspace_id
        .contains(directory.path().to_str().unwrap()));
    let encoded = serde_json::to_string(&grant).unwrap();
    assert!(!encoded.contains(directory.path().to_str().unwrap()));
    assert_eq!(service.verify(&grant.workspace_id, "main").unwrap(), grant);
    assert_eq!(
        service.verify(&grant.workspace_id, "other").unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
}

#[test]
fn cloned_service_shares_the_exact_authority_registry() {
    let directory = tempfile::tempdir().unwrap();
    let service = DesktopWorkspaceService::default();
    let runtime_view = service.clone();
    let grant = service.admit_selected(directory.path(), "main").unwrap();

    assert_eq!(
        runtime_view.verify(&grant.workspace_id, "main").unwrap(),
        grant
    );
    runtime_view
        .revoke(&grant.workspace_id, grant.grant_revision, "main")
        .unwrap();
    assert_eq!(
        service.verify(&grant.workspace_id, "main").unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
}

#[test]
fn write_authority_requires_the_exact_selected_workspace_identity() {
    let directory = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    assert_eq!(
        service
            .authorize_writes(&grant.workspace_id, other.path(), "main")
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    let writable = service
        .authorize_writes(&grant.workspace_id, directory.path(), "main")
        .unwrap();
    assert_eq!(writable.workspace_id, grant.workspace_id);
    assert_eq!(writable.grant_revision, grant.grant_revision + 1);
    assert_eq!(writable.access, "read_write");
}

#[cfg(target_os = "macos")]
#[test]
fn native_bookmark_restores_the_same_opaque_workspace_after_process_rebuild() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("Project");
    fs::create_dir(&workspace).unwrap();
    fs::write(workspace.join("brief.md"), "durable context").unwrap();
    let manifest = root.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
    let store = Arc::new(MemoryBookmarkStore::default());

    let original = DesktopWorkspaceService::durable(manifest.clone(), store.clone());
    let grant = original.admit_selected(&workspace, "main").unwrap();
    drop(original);

    let restored = DesktopWorkspaceService::durable(manifest.clone(), store.clone());
    assert_eq!(restored.recover("main").unwrap(), 1);
    let recovered_grant = restored.verify(&grant.workspace_id, "main").unwrap();
    assert_eq!(recovered_grant.workspace_id, grant.workspace_id);
    assert_eq!(recovered_grant.display_name, "Project");
    let page = restored
        .list_entries(&grant.workspace_id, "main", None, None, 8)
        .unwrap();
    assert_eq!(page.entries[0].display_name, "brief.md");

    restored
        .revoke(&grant.workspace_id, grant.grant_revision, "main")
        .unwrap();
    assert_eq!(
        DesktopWorkspaceService::durable(manifest, store)
            .recover("main")
            .unwrap(),
        0
    );
}

#[cfg(target_os = "macos")]
#[test]
fn missing_private_bookmark_is_reported_without_blocking_other_recovery() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("Project");
    fs::create_dir(&workspace).unwrap();
    let manifest = root.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
    let store = Arc::new(MemoryBookmarkStore::default());
    let original = DesktopWorkspaceService::durable(manifest.clone(), store.clone());
    original.admit_selected(&workspace, "main").unwrap();
    drop(original);
    store.0.lock().unwrap().clear();

    let restored = DesktopWorkspaceService::durable(manifest, store);
    assert_eq!(restored.recover("main").unwrap(), 0);
    let status = restored.recovery_status().unwrap();
    assert_eq!(status.state, "attention_required");
    assert_eq!(status.restored_count, 0);
    assert_eq!(status.needs_reauthorization_count, 1);
    let authorization = restored.authorizations().unwrap()[0].clone();
    let receipt = restored
        .revoke(
            &authorization.workspace_id,
            authorization.grant_revision,
            "main",
        )
        .unwrap();
    assert_eq!(receipt.outcome, "revoked");
    assert!(restored.authorizations().unwrap().is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn reauthorization_preserves_identity_rejects_wrong_roots_and_keeps_dormant_records() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("Original");
    let wrong = root.path().join("Wrong");
    let additional = root.path().join("Additional");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&wrong).unwrap();
    fs::create_dir(&additional).unwrap();
    fs::write(workspace.join("brief.md"), "restored").unwrap();
    let manifest = root.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
    let store = Arc::new(MemoryBookmarkStore::default());
    let original = DesktopWorkspaceService::durable(manifest.clone(), store.clone());
    let grant = original.admit_selected(&workspace, "main").unwrap();
    drop(original);
    store.0.lock().unwrap().clear();

    let restored = DesktopWorkspaceService::durable(manifest, store);
    assert_eq!(restored.recover("main").unwrap(), 0);
    assert_eq!(
        restored.authorizations().unwrap()[0].state,
        "needs_reauthorization"
    );
    assert_eq!(
        restored
            .reauthorize(&grant.workspace_id, &wrong, "main")
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    let additional_grant = restored.admit_selected(&additional, "main").unwrap();
    assert_eq!(restored.authorizations().unwrap().len(), 2);

    let renewed = restored
        .reauthorize(&grant.workspace_id, &workspace, "main")
        .unwrap();
    assert_eq!(renewed.workspace_id, grant.workspace_id);
    assert_eq!(renewed.grant_revision, grant.grant_revision + 1);
    assert_eq!(restored.recovery_status().unwrap().state, "ready");
    let authorizations = restored.authorizations().unwrap();
    assert!(authorizations.iter().all(|item| item.state == "active"));
    assert!(authorizations
        .iter()
        .any(|item| item.workspace_id == additional_grant.workspace_id));
    assert_eq!(
        restored
            .list_entries(&renewed.workspace_id, "main", None, None, 8)
            .unwrap()
            .entries[0]
            .display_name,
        "brief.md"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_selection_reuses_active_or_dormant_workspace_identity() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("Project");
    fs::create_dir(&workspace).unwrap();
    let manifest = root.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
    let store = Arc::new(MemoryBookmarkStore::default());
    let original = DesktopWorkspaceService::durable(manifest.clone(), store.clone());
    let grant = original.admit_selected(&workspace, "main").unwrap();
    let duplicate = original.admit_selected(&workspace, "main").unwrap();
    assert_eq!(duplicate.workspace_id, grant.workspace_id);
    assert_eq!(duplicate.grant_revision, grant.grant_revision);
    drop(original);
    store.0.lock().unwrap().clear();

    let restored = DesktopWorkspaceService::durable(manifest, store);
    assert_eq!(restored.recover("main").unwrap(), 0);
    let renewed = restored.admit_selected(&workspace, "main").unwrap();
    assert_eq!(renewed.workspace_id, grant.workspace_id);
    assert_eq!(renewed.grant_revision, grant.grant_revision + 1);
    assert_eq!(restored.authorizations().unwrap().len(), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn filesystem_and_home_roots_are_rejected_as_overbroad_authority() {
    let service = DesktopWorkspaceService::default();
    assert_eq!(
        service
            .admit_selected(std::path::Path::new("/"), "main")
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    assert_eq!(
        service
            .admit_selected(&garive_macos_bookmark::home_directory(), "main")
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
}

#[test]
fn revocation_drops_private_authority_without_falsifying_the_public_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    let receipt = service
        .revoke(&grant.workspace_id, grant.grant_revision, "main")
        .unwrap();
    assert_eq!(receipt.workspace_id, grant.workspace_id);
    assert_eq!(receipt.grant_revision, grant.grant_revision);
    assert_eq!(receipt.outcome, "revoked");
    assert!(!receipt.cleanup_pending);
    assert_eq!(
        service.verify(&grant.workspace_id, "main").unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    assert_eq!(grant.state, "active");
}

#[cfg(target_os = "macos")]
#[test]
fn revocation_receipt_survives_restart_and_retries_private_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("Project");
    fs::create_dir(&workspace).unwrap();
    let manifest = root.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
    let store = Arc::new(MemoryBookmarkStore::default());
    let original = DesktopWorkspaceService::durable(manifest.clone(), store.clone());
    let grant = original.admit_selected(&workspace, "main").unwrap();
    store.1.store(true, Ordering::SeqCst);

    let receipt = original
        .revoke(&grant.workspace_id, grant.grant_revision, "main")
        .unwrap();
    assert_eq!(receipt.outcome, "revoked");
    assert!(receipt.cleanup_pending);
    assert!(original.authorizations().unwrap().is_empty());
    drop(original);

    let restarted = DesktopWorkspaceService::durable(manifest, store.clone());
    assert_eq!(restarted.recover("main").unwrap(), 0);
    let replay = restarted
        .revoke(&grant.workspace_id, grant.grant_revision, "main")
        .unwrap();
    assert_eq!(replay.outcome, "already_revoked");
    assert!(replay.cleanup_pending);

    store.1.store(false, Ordering::SeqCst);
    assert_eq!(restarted.recover("main").unwrap(), 0);
    let cleaned = restarted
        .revoke(&grant.workspace_id, grant.grant_revision, "main")
        .unwrap();
    assert_eq!(cleaned.outcome, "already_revoked");
    assert!(!cleaned.cleanup_pending);
}

#[cfg(unix)]
#[test]
fn symlink_roots_are_rejected_before_capability_allocation() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let link = directory.path().join("linked-root");
    symlink(directory.path(), &link).unwrap();
    assert_eq!(
        DesktopWorkspaceService::default()
            .admit_selected(&link, "main")
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
}

#[test]
fn entry_pages_are_bounded_path_free_and_support_directory_descent() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("alpha.md"), "alpha").unwrap();
    fs::write(directory.path().join("bravo.txt"), "bravo").unwrap();
    fs::write(directory.path().join(".private"), "secret").unwrap();
    fs::create_dir(directory.path().join("notes")).unwrap();
    fs::write(directory.path().join("notes").join("inside.md"), "inside").unwrap();
    fs::create_dir(directory.path().join("Sample.app")).unwrap();

    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    let first = service
        .list_entries(&grant.workspace_id, "main", None, None, 2)
        .unwrap();
    assert_eq!(first.entries.len(), 2);
    assert!(first.has_more);
    assert!(first.next_cursor.is_some());
    let second = service
        .list_entries(
            &grant.workspace_id,
            "main",
            None,
            first.next_cursor.as_deref(),
            2,
        )
        .unwrap();
    let mut entries = first.entries;
    entries.extend(second.entries);
    assert!(entries.iter().all(|entry| entry.display_name != ".private"));
    assert!(!serde_json::to_string(&entries)
        .unwrap()
        .contains(directory.path().to_string_lossy().as_ref()));
    let package = entries
        .iter()
        .find(|entry| entry.display_name == "Sample.app")
        .unwrap();
    assert!(!package.selectable);

    let notes = entries
        .iter()
        .find(|entry| entry.display_name == "notes")
        .unwrap();
    let nested = service
        .list_entries(&grant.workspace_id, "main", Some(&notes.entry_id), None, 8)
        .unwrap();
    assert_eq!(nested.entries[0].display_name, "inside.md");
    assert_eq!(
        nested.entries[0].parent_entry_id.as_deref(),
        Some(&*notes.entry_id)
    );
}

#[cfg(unix)]
#[test]
fn enumeration_omits_symlinks_and_rejects_forged_cursors_and_parents() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("safe.txt"), "safe").unwrap();
    symlink(
        directory.path().join("safe.txt"),
        directory.path().join("link.txt"),
    )
    .unwrap();
    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    let page = service
        .list_entries(&grant.workspace_id, "main", None, None, 8)
        .unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].display_name, "safe.txt");
    assert_eq!(
        service
            .list_entries(&grant.workspace_id, "main", None, Some("cursor-forged"), 8)
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    assert_eq!(
        service
            .list_entries(&grant.workspace_id, "main", Some("entry-forged"), None, 8)
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
}

#[test]
fn selected_text_is_read_only_from_a_previously_enumerated_opaque_entry() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("brief.md"), "hello 世界").unwrap();
    fs::write(directory.path().join("binary.bin"), [0xff, 0x00]).unwrap();
    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    let page = service
        .list_entries(&grant.workspace_id, "main", None, None, 8)
        .unwrap();
    let brief = page
        .entries
        .iter()
        .find(|entry| entry.display_name == "brief.md")
        .unwrap();
    let context = service
        .read_context_files(
            &grant.workspace_id,
            "main",
            std::slice::from_ref(&brief.entry_id),
        )
        .unwrap();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content_utf8, "hello 世界");
    assert_eq!(context[0].workspace_id, grant.workspace_id);
    assert_eq!(context[0].grant_revision, 1);
    assert_eq!(context[0].content_digest.len(), 64);

    let binary = page
        .entries
        .iter()
        .find(|entry| entry.display_name == "binary.bin")
        .unwrap();
    assert_eq!(
        service
            .read_context_files(
                &grant.workspace_id,
                "main",
                std::slice::from_ref(&binary.entry_id),
            )
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    assert_eq!(
        service
            .read_context_files(&grant.workspace_id, "main", &["entry-forged".into()])
            .unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
}
