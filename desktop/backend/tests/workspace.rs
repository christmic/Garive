use std::fs;

use garive_desktop::{DesktopWorkspaceError, DesktopWorkspaceService};

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
fn revocation_drops_private_authority_without_falsifying_the_public_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let service = DesktopWorkspaceService::default();
    let grant = service.admit_selected(directory.path(), "main").unwrap();
    service.revoke(&grant.workspace_id, "main").unwrap();
    assert_eq!(
        service.verify(&grant.workspace_id, "main").unwrap_err(),
        DesktopWorkspaceError::CapabilityInvalid
    );
    assert_eq!(grant.state, "active");
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
