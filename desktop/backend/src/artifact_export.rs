use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_EXPORT_TARGETS: usize = 16;
const MAX_ARTIFACT_BYTES: usize = 256 * 1_024;
const EXPORT_TARGET_LIFETIME_SECONDS: u64 = 300;
const MAX_EXPORT_JOURNAL_BYTES: usize = 8 * 1_024;
const EXPORT_JOURNAL_TEMP_FILE: &str = ".desktop-artifact-exports.tmp";
/// Path-free private journal containing only pending export target identities.
pub const DESKTOP_ARTIFACT_EXPORT_JOURNAL_FILE: &str = "desktop-artifact-exports.json";

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
    pending: Arc<Mutex<BTreeSet<String>>>,
    journal_path: Option<Arc<PathBuf>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExportJournal {
    schema_version: u32,
    pending_target_ids: Vec<String>,
}

impl DesktopArtifactExportService {
    /// Restores a bounded path-free crash-cleanup journal.
    pub fn durable(journal_path: PathBuf) -> Result<Self, DesktopArtifactExportError> {
        let pending = read_journal(&journal_path)?;
        Ok(Self {
            targets: Arc::default(),
            pending: Arc::new(Mutex::new(pending)),
            journal_path: Some(Arc::new(journal_path)),
        })
    }

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
        self.cleanup_pending_in(&directory)?;
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
        self.begin_pending(export_target_id)?;
        let result = atomic_create(&target, export_target_id, bytes);
        let _ = self.finish_pending(export_target_id);
        result?;
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

    fn begin_pending(&self, export_target_id: &str) -> Result<(), DesktopArtifactExportError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| DesktopArtifactExportError::Unavailable)?;
        let mut next = pending.clone();
        if !next.insert(export_target_id.to_owned()) || next.len() > MAX_EXPORT_TARGETS {
            return Err(DesktopArtifactExportError::Invalid);
        }
        self.persist_pending(&next)?;
        *pending = next;
        Ok(())
    }

    fn finish_pending(&self, export_target_id: &str) -> Result<(), DesktopArtifactExportError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| DesktopArtifactExportError::Unavailable)?;
        let mut next = pending.clone();
        next.remove(export_target_id);
        self.persist_pending(&next)?;
        *pending = next;
        Ok(())
    }

    #[cfg(unix)]
    fn cleanup_pending_in(&self, directory: &File) -> Result<(), DesktopArtifactExportError> {
        use rustix::fs::AtFlags;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| DesktopArtifactExportError::Unavailable)?;
        let mut next = pending.clone();
        for target_id in pending.iter() {
            let name = temporary_name(target_id);
            match rustix::fs::unlinkat(directory, name.as_str(), AtFlags::empty()) {
                Ok(()) | Err(rustix::io::Errno::NOENT) => {
                    next.remove(target_id);
                }
                Err(_) => {}
            }
        }
        if next != *pending {
            self.persist_pending(&next)?;
            *pending = next;
        }
        Ok(())
    }

    fn persist_pending(
        &self,
        pending: &BTreeSet<String>,
    ) -> Result<(), DesktopArtifactExportError> {
        let Some(path) = &self.journal_path else {
            return Ok(());
        };
        write_journal(path, pending)
    }
}

#[cfg(unix)]
fn atomic_create(
    target: &ExportTarget,
    export_target_id: &str,
    bytes: &[u8],
) -> Result<(), DesktopArtifactExportError> {
    use rustix::fs::{AtFlags, Mode, OFlags};
    let temporary = temporary_name(export_target_id);
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
fn atomic_create(_: &ExportTarget, _: &str, _: &[u8]) -> Result<(), DesktopArtifactExportError> {
    Err(DesktopArtifactExportError::Unavailable)
}

fn temporary_name(export_target_id: &str) -> String {
    format!(".garive-export-{export_target_id}.tmp")
}

fn read_journal(path: &Path) -> Result<BTreeSet<String>, DesktopArtifactExportError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(_) => return Err(DesktopArtifactExportError::Unavailable),
    };
    if bytes.len() > MAX_EXPORT_JOURNAL_BYTES {
        return Err(DesktopArtifactExportError::BoundExceeded);
    }
    let journal: ExportJournal =
        serde_json::from_slice(&bytes).map_err(|_| DesktopArtifactExportError::Unavailable)?;
    if journal.schema_version != 1 || journal.pending_target_ids.len() > MAX_EXPORT_TARGETS {
        return Err(DesktopArtifactExportError::BoundExceeded);
    }
    let pending = journal
        .pending_target_ids
        .into_iter()
        .collect::<BTreeSet<_>>();
    if pending.len() > MAX_EXPORT_TARGETS || pending.iter().any(|value| !valid_target_id(value)) {
        return Err(DesktopArtifactExportError::Invalid);
    }
    Ok(pending)
}

fn write_journal(
    path: &Path,
    pending: &BTreeSet<String>,
) -> Result<(), DesktopArtifactExportError> {
    if pending.len() > MAX_EXPORT_TARGETS || pending.iter().any(|value| !valid_target_id(value)) {
        return Err(DesktopArtifactExportError::Invalid);
    }
    let bytes = serde_json::to_vec(&ExportJournal {
        schema_version: 1,
        pending_target_ids: pending.iter().cloned().collect(),
    })
    .map_err(|_| DesktopArtifactExportError::Unavailable)?;
    if bytes.len() > MAX_EXPORT_JOURNAL_BYTES {
        return Err(DesktopArtifactExportError::BoundExceeded);
    }
    let parent = path
        .parent()
        .ok_or(DesktopArtifactExportError::Unavailable)?;
    fs::create_dir_all(parent).map_err(|_| DesktopArtifactExportError::Unavailable)?;
    let temporary = parent.join(EXPORT_JOURNAL_TEMP_FILE);
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| DesktopArtifactExportError::Unavailable)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| DesktopArtifactExportError::Unavailable)?;
    fs::rename(&temporary, path).map_err(|_| DesktopArtifactExportError::Unavailable)?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| DesktopArtifactExportError::Unavailable)?;
    Ok(())
}

fn valid_target_id(value: &str) -> bool {
    value.starts_with("export-target-")
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
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

    #[test]
    fn durable_path_free_journal_cleans_only_its_interrupted_temporary() {
        let config = tempfile::tempdir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let journal = config.path().join(DESKTOP_ARTIFACT_EXPORT_JOURNAL_FILE);
        let destination = directory.path().join("copy.md");
        let service = DesktopArtifactExportService::durable(journal.clone()).unwrap();
        let target = service.admit_selected(&destination, "main").unwrap();
        service.begin_pending(&target.export_target_id).unwrap();
        let interrupted = directory
            .path()
            .join(temporary_name(&target.export_target_id));
        std::fs::write(&interrupted, "partial private export").unwrap();
        let encoded = std::fs::read_to_string(&journal).unwrap();
        assert!(!encoded.contains(directory.path().to_string_lossy().as_ref()));
        drop(service);

        let restarted = DesktopArtifactExportService::durable(journal.clone()).unwrap();
        let unrelated = directory.path().join("keep.txt");
        std::fs::write(&unrelated, "user data").unwrap();
        restarted
            .admit_selected(&directory.path().join("new-copy.md"), "main")
            .unwrap();
        assert!(!interrupted.exists());
        assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "user data");
        assert!(!std::fs::read_to_string(journal)
            .unwrap()
            .contains(&target.export_target_id));
    }
}
