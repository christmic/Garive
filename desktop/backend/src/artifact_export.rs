use std::{
    collections::BTreeMap,
    fs::File,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_EXPORT_TARGETS: usize = 16;
const MAX_ARTIFACT_BYTES: usize = 256 * 1_024;
const EXPORT_TARGET_LIFETIME_SECONDS: u64 = 300;

/// One path-free, process-local destination selected through the native save panel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopArtifactExportTarget {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Opaque one-shot target capability identity.
    pub export_target_id: String,
    /// Safe final-component label selected by the operator.
    pub display_name: String,
    /// Exact process-local lifecycle state.
    pub state: &'static str,
    /// Canonical UTC expiry instant.
    pub expires_at: String,
}

/// Durable-source-bound receipt for one completed local export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopArtifactExportReceipt {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Stable source Artifact identity.
    pub artifact_id: String,
    /// Exact immutable source revision.
    pub revision: u64,
    /// Safe destination final-component label.
    pub display_name: String,
    /// Exact exported byte count.
    pub byte_size: u64,
    /// SHA-256 digest verified before export.
    pub content_digest: String,
    /// Exact terminal export state.
    pub state: &'static str,
}

/// Stable failure classes for target admission and one-shot Artifact export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopArtifactExportError {
    /// Request or capability coordinates were invalid.
    Invalid,
    /// Selected destination or local I/O was unavailable.
    Unavailable,
    /// Export refused to overwrite an existing destination.
    TargetExists,
    /// Process-local target or byte bounds were exceeded.
    BoundExceeded,
}

impl DesktopArtifactExportError {
    /// Returns one stable frontend-safe error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::TargetExists => "artifact_overwrite_required",
            Self::Invalid | Self::Unavailable | Self::BoundExceeded => "artifact_export_stale",
        }
    }
}

struct ExportTarget {
    display_name: String,
    owner_window: String,
    expires_at_unix: u64,
    #[cfg(unix)]
    directory: File,
}

/// Process-local registry of one-shot export destinations. Clones share exact authority.
#[derive(Clone, Default)]
pub struct DesktopArtifactExportService {
    targets: Arc<Mutex<BTreeMap<String, ExportTarget>>>,
}

impl DesktopArtifactExportService {
    /// Admits one native-save-panel selection as a bounded path-free capability.
    pub fn admit_selected(
        &self,
        path: &Path,
        owner_window: &str,
    ) -> Result<DesktopArtifactExportTarget, DesktopArtifactExportError> {
        let now = unix_now()?;
        if owner_window.is_empty() || path.exists() {
            return Err(if path.exists() {
                DesktopArtifactExportError::TargetExists
            } else {
                DesktopArtifactExportError::Invalid
            });
        }
        let display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| valid_name(name))
            .ok_or(DesktopArtifactExportError::Invalid)?
            .to_owned();
        let parent = path.parent().ok_or(DesktopArtifactExportError::Invalid)?;
        #[cfg(unix)]
        let directory = File::from(
            rustix::fs::open(
                parent,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|_| DesktopArtifactExportError::Unavailable)?,
        );
        #[cfg(not(unix))]
        return Err(DesktopArtifactExportError::Unavailable);
        let expires_at_unix = now.saturating_add(EXPORT_TARGET_LIFETIME_SECONDS);
        let export_target_id = format!("export-target-{}", Uuid::new_v4());
        let mut targets = self
            .targets
            .lock()
            .map_err(|_| DesktopArtifactExportError::Unavailable)?;
        targets.retain(|_, target| target.expires_at_unix > now);
        if targets.len() >= MAX_EXPORT_TARGETS {
            return Err(DesktopArtifactExportError::BoundExceeded);
        }
        targets.insert(
            export_target_id.clone(),
            ExportTarget {
                display_name: display_name.clone(),
                owner_window: owner_window.into(),
                expires_at_unix,
                directory,
            },
        );
        Ok(DesktopArtifactExportTarget {
            schema_version: 1,
            export_target_id,
            display_name,
            state: "ready",
            expires_at: timestamp(expires_at_unix)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    /// Consumes one exact target capability and atomically creates the exported copy.
    pub fn export(
        &self,
        export_target_id: &str,
        owner_window: &str,
        artifact_id: &str,
        revision: u64,
        content_digest: &str,
        bytes: &[u8],
    ) -> Result<DesktopArtifactExportReceipt, DesktopArtifactExportError> {
        let now = unix_now()?;
        let target = self
            .targets
            .lock()
            .map_err(|_| DesktopArtifactExportError::Unavailable)?
            .remove(export_target_id)
            .ok_or(DesktopArtifactExportError::Invalid)?;
        if target.owner_window != owner_window
            || target.expires_at_unix <= now
            || artifact_id.is_empty()
            || revision == 0
            || bytes.len() > MAX_ARTIFACT_BYTES
            || digest(bytes) != content_digest
        {
            return Err(DesktopArtifactExportError::Invalid);
        }
        atomic_create(&target, bytes)?;
        Ok(DesktopArtifactExportReceipt {
            schema_version: 1,
            artifact_id: artifact_id.into(),
            revision,
            display_name: target.display_name,
            byte_size: bytes.len() as u64,
            content_digest: content_digest.into(),
            state: "exported",
        })
    }
}

#[cfg(unix)]
fn atomic_create(target: &ExportTarget, bytes: &[u8]) -> Result<(), DesktopArtifactExportError> {
    use rustix::fs::{AtFlags, Mode, OFlags};
    let temporary = format!(".garive-export-{}.tmp", Uuid::new_v4());
    let descriptor = rustix::fs::openat(
        &target.directory,
        temporary.as_str(),
        OFlags::CREATE | OFlags::EXCL | OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|_| DesktopArtifactExportError::Unavailable)?;
    let mut file = File::from(descriptor);
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        let _ = rustix::fs::unlinkat(&target.directory, temporary.as_str(), AtFlags::empty());
        return Err(DesktopArtifactExportError::Unavailable);
    }
    if rustix::fs::linkat(
        &target.directory,
        temporary.as_str(),
        &target.directory,
        target.display_name.as_str(),
        AtFlags::empty(),
    )
    .is_err()
    {
        let _ = rustix::fs::unlinkat(&target.directory, temporary.as_str(), AtFlags::empty());
        return Err(DesktopArtifactExportError::TargetExists);
    }
    let _ = rustix::fs::unlinkat(&target.directory, temporary.as_str(), AtFlags::empty());
    rustix::fs::fsync(&target.directory).map_err(|_| DesktopArtifactExportError::Unavailable)
}

#[cfg(not(unix))]
fn atomic_create(_: &ExportTarget, _: &[u8]) -> Result<(), DesktopArtifactExportError> {
    Err(DesktopArtifactExportError::Unavailable)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn unix_now() -> Result<u64, DesktopArtifactExportError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| DesktopArtifactExportError::Unavailable)
}

fn timestamp(unix: u64) -> Result<String, DesktopArtifactExportError> {
    DateTime::<Utc>::from_timestamp(unix as i64, 0)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
        .ok_or(DesktopArtifactExportError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_target_is_path_free_atomic_and_one_shot() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("copy.md");
        let service = DesktopArtifactExportService::default();
        let target = service.admit_selected(&destination, "main").unwrap();
        assert!(!serde_json::to_string(&target)
            .unwrap()
            .contains(directory.path().to_string_lossy().as_ref()));
        let bytes = b"verified artifact";
        let receipt = service
            .export(
                &target.export_target_id,
                "main",
                "artifact-1",
                1,
                &digest(bytes),
                bytes,
            )
            .unwrap();
        assert_eq!(std::fs::read(destination).unwrap(), bytes);
        assert_eq!(receipt.state, "exported");
        assert_eq!(
            service.export(
                &target.export_target_id,
                "main",
                "artifact-1",
                1,
                &digest(bytes),
                bytes,
            ),
            Err(DesktopArtifactExportError::Invalid)
        );
    }

    #[test]
    fn export_never_overwrites_an_existing_target() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("existing.md");
        std::fs::write(&destination, "original").unwrap();
        let service = DesktopArtifactExportService::default();
        assert!(matches!(
            service.admit_selected(&destination, "main"),
            Err(DesktopArtifactExportError::TargetExists)
        ));
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "original");
    }

    #[test]
    fn destination_race_preserves_existing_bytes_and_cleans_temporary() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("raced.md");
        let service = DesktopArtifactExportService::default();
        let target = service.admit_selected(&destination, "main").unwrap();
        std::fs::write(&destination, "concurrent").unwrap();
        let bytes = b"artifact";
        assert_eq!(
            service.export(
                &target.export_target_id,
                "main",
                "artifact-1",
                1,
                &digest(bytes),
                bytes,
            ),
            Err(DesktopArtifactExportError::TargetExists)
        );
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "concurrent");
        assert!(std::fs::read_dir(directory.path())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".garive-export-")));
    }

    #[test]
    fn wrong_owner_or_digest_consumes_authority_without_creating_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("copy.md");
        let service = DesktopArtifactExportService::default();
        let target = service.admit_selected(&destination, "main").unwrap();
        assert_eq!(
            service.export(
                &target.export_target_id,
                "other",
                "artifact-1",
                1,
                &digest(b"artifact"),
                b"artifact",
            ),
            Err(DesktopArtifactExportError::Invalid)
        );
        assert!(!destination.exists());
        assert_eq!(
            service.export(
                &target.export_target_id,
                "main",
                "artifact-1",
                1,
                &digest(b"artifact"),
                b"artifact",
            ),
            Err(DesktopArtifactExportError::Invalid)
        );
    }
}
