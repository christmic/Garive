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
