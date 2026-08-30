use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

use crate::DesktopWorkspaceError;

/// Operating-system credential service containing only Workspace bookmark bytes.
pub const DESKTOP_WORKSPACE_BOOKMARK_SERVICE: &str = "dev.garive.desktop.workspace-bookmark";
/// Bounded path-free Workspace recovery index in the app configuration directory.
pub const DESKTOP_WORKSPACE_MANIFEST_FILE: &str = "desktop-workspaces.json";

const MAX_BOOKMARK_BYTES: usize = 128 * 1_024;
const MAX_MANIFEST_BYTES: usize = 64 * 1_024;
const MAX_MANIFEST_RECORDS: usize = 64;
const MANIFEST_TEMP_FILE: &str = ".desktop-workspaces.tmp";

/// Private write/read/delete store for opaque native bookmark bytes.
pub trait DesktopWorkspaceBookmarkStore: Send + Sync {
    /// Stores one bounded bookmark under an opaque backend-only reference.
    fn store(&self, bookmark_ref: &str, bytes: &[u8]) -> Result<(), DesktopWorkspaceError>;
    /// Loads one exact bounded bookmark for backend-only recovery.
    fn load(&self, bookmark_ref: &str) -> Result<Vec<u8>, DesktopWorkspaceError>;
    /// Deletes one exact bookmark when its authority is revoked.
    fn delete(&self, bookmark_ref: &str) -> Result<(), DesktopWorkspaceError>;
}

/// Shipping Workspace bookmark store backed by the operating-system credential vault.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemDesktopWorkspaceBookmarkStore;

impl DesktopWorkspaceBookmarkStore for SystemDesktopWorkspaceBookmarkStore {
    fn store(&self, bookmark_ref: &str, bytes: &[u8]) -> Result<(), DesktopWorkspaceError> {
        validate_ref(bookmark_ref)?;
        if bytes.is_empty() || bytes.len() > MAX_BOOKMARK_BYTES {
            return Err(DesktopWorkspaceError::BoundExceeded);
        }
        let value = STANDARD.encode(bytes);
        let entry = keyring::Entry::new(DESKTOP_WORKSPACE_BOOKMARK_SERVICE, bookmark_ref)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        entry
            .set_password(&value)
            .map_err(|_| DesktopWorkspaceError::Unavailable)
    }

    fn load(&self, bookmark_ref: &str) -> Result<Vec<u8>, DesktopWorkspaceError> {
        validate_ref(bookmark_ref)?;
        let entry = keyring::Entry::new(DESKTOP_WORKSPACE_BOOKMARK_SERVICE, bookmark_ref)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let value = entry
            .get_password()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        if value.len() > MAX_BOOKMARK_BYTES.saturating_mul(2) {
            return Err(DesktopWorkspaceError::BoundExceeded);
        }
        let bytes = STANDARD
            .decode(value)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        if bytes.is_empty() || bytes.len() > MAX_BOOKMARK_BYTES {
            return Err(DesktopWorkspaceError::BoundExceeded);
        }
        Ok(bytes)
    }

    fn delete(&self, bookmark_ref: &str) -> Result<(), DesktopWorkspaceError> {
        validate_ref(bookmark_ref)?;
        let entry = keyring::Entry::new(DESKTOP_WORKSPACE_BOOKMARK_SERVICE, bookmark_ref)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(DesktopWorkspaceError::Unavailable),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceManifestRecord {
    pub schema_version: u32,
    pub workspace_id: String,
    pub display_name: String,
    pub grant_revision: u64,
    pub bookmark_ref: String,
    pub device: u64,
    pub file: u64,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub cleanup_pending: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceManifest {
    schema_version: u32,
    workspaces: Vec<WorkspaceManifestRecord>,
}

pub(crate) fn read_manifest(
    path: &Path,
) -> Result<Vec<WorkspaceManifestRecord>, DesktopWorkspaceError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(DesktopWorkspaceError::Unavailable),
    };
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(DesktopWorkspaceError::BoundExceeded);
    }
    let manifest: WorkspaceManifest =
        serde_json::from_slice(&bytes).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    if !matches!(manifest.schema_version, 1 | 2) || manifest.workspaces.len() > MAX_MANIFEST_RECORDS
    {
        return Err(DesktopWorkspaceError::BoundExceeded);
    }
    for record in &manifest.workspaces {
        validate_record(record)?;
    }
    Ok(manifest.workspaces)
}

pub(crate) fn write_manifest(
    path: &Path,
    records: Vec<WorkspaceManifestRecord>,
) -> Result<(), DesktopWorkspaceError> {
    if records.len() > MAX_MANIFEST_RECORDS {
        return Err(DesktopWorkspaceError::BoundExceeded);
    }
    for record in &records {
        validate_record(record)?;
    }
    let bytes = serde_json::to_vec(&WorkspaceManifest {
        schema_version: 2,
        workspaces: records,
    })
    .map_err(|_| DesktopWorkspaceError::Unavailable)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(DesktopWorkspaceError::BoundExceeded);
    }
    let parent = path.parent().ok_or(DesktopWorkspaceError::Unavailable)?;
    fs::create_dir_all(parent).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    let temporary = parent.join(MANIFEST_TEMP_FILE);
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| DesktopWorkspaceError::Unavailable)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| DesktopWorkspaceError::Unavailable)?;
    fs::rename(&temporary, path).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    Ok(())
}

fn validate_record(record: &WorkspaceManifestRecord) -> Result<(), DesktopWorkspaceError> {
    if record.schema_version != 1
        || !record.workspace_id.starts_with("workspace-")
        || record.workspace_id.len() > 64
        || record.display_name.is_empty()
        || record.display_name.len() > 128
        || record.grant_revision == 0
    {
        return Err(DesktopWorkspaceError::Unavailable);
    }
    validate_ref(&record.bookmark_ref)
}

fn validate_ref(value: &str) -> Result<(), DesktopWorkspaceError> {
    if value.starts_with("bookmark-")
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(DesktopWorkspaceError::CapabilityInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trip_contains_no_path_or_bookmark_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
        let record = WorkspaceManifestRecord {
            schema_version: 1,
            workspace_id: "workspace-123".into(),
            display_name: "Project".into(),
            grant_revision: 1,
            bookmark_ref: "bookmark-456".into(),
            device: 7,
            file: 9,
            revoked: false,
            cleanup_pending: false,
        };
        write_manifest(&path, vec![record.clone()]).unwrap();
        let encoded = fs::read_to_string(&path).unwrap();
        assert!(!encoded.contains(directory.path().to_string_lossy().as_ref()));
        assert_eq!(read_manifest(&path).unwrap(), vec![record]);
    }

    #[test]
    fn legacy_active_records_migrate_without_broadening_authority() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
        fs::write(&path, br#"{"schema_version":1,"workspaces":[{"schema_version":1,"workspace_id":"workspace-123","display_name":"Project","grant_revision":1,"bookmark_ref":"bookmark-456","device":7,"file":9}]}"#).unwrap();
        let records = read_manifest(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert!(!records[0].revoked);
        write_manifest(&path, records).unwrap();
        assert!(fs::read_to_string(path)
            .unwrap()
            .starts_with("{\"schema_version\":2,"));
    }

    #[test]
    fn malformed_or_oversized_manifest_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(DESKTOP_WORKSPACE_MANIFEST_FILE);
        fs::write(
            &path,
            br#"{"schema_version":1,"workspaces":[],"path":"/tmp"}"#,
        )
        .unwrap();
        assert_eq!(
            read_manifest(&path),
            Err(DesktopWorkspaceError::Unavailable)
        );
        fs::write(&path, vec![b'x'; MAX_MANIFEST_BYTES + 1]).unwrap();
        assert_eq!(
            read_manifest(&path),
            Err(DesktopWorkspaceError::BoundExceeded)
        );
    }
}
