use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::workspace_bookmark::{
    read_manifest, write_manifest, DesktopWorkspaceBookmarkStore, WorkspaceManifestRecord,
};

const MAX_ACTIVE_WORKSPACES: usize = 16;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_ENTRIES_PER_PAGE: usize = 64;
const MAX_SCANNED_ENTRIES: usize = 512;
const MAX_CACHED_ENTRIES: usize = 4_096;
const MAX_CONTEXT_FILES: usize = 8;
const MAX_CONTEXT_FILE_BYTES: usize = 48 * 1_024;
const MAX_CONTEXT_TOTAL_BYTES: usize = 60 * 1_024;
const WORKSPACE_LIFETIME_SECONDS: u64 = 1_800;

/// Opaque path-free public view of one process-local Workspace selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopWorkspaceGrant {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Opaque capability identity.
    pub workspace_id: String,
    /// Bounded final-component label safe for presentation.
    pub display_name: String,
    /// V1 selection admits enumeration posture only.
    pub access: &'static str,
    /// Monotonic capability revision bound by Session attachment.
    pub grant_revision: u64,
    /// Public lifecycle state.
    pub state: &'static str,
    /// Canonical UTC expiry instant.
    pub expires_at: String,
}

/// One opaque path-free direct child of a Workspace directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopWorkspaceEntry {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Opaque process-local entry capability.
    pub entry_id: String,
    /// Opaque parent identity, absent for direct root children.
    pub parent_entry_id: Option<String>,
    /// Bounded presentation-only final component.
    pub display_name: String,
    /// Safe coarse content class.
    pub kind: &'static str,
    /// File size when the child is a regular file.
    pub byte_size: Option<u64>,
    /// Whether context selection or directory descent is permitted.
    pub selectable: bool,
}

/// One bounded stable-order page of Workspace entries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopWorkspaceEntryPage {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Opaque Workspace identity owning every returned entry.
    pub workspace_id: String,
    /// Opaque directory identity represented by this page.
    pub parent_entry_id: Option<String>,
    /// Bounded stable-order public entries.
    pub entries: Vec<DesktopWorkspaceEntry>,
    /// Opaque continuation token when another page exists.
    pub next_cursor: Option<String>,
    /// Whether the same directory has another page.
    pub has_more: bool,
}

/// Backend-only selected file content passed directly into Runtime admission.
///
/// Deliberately does not implement `Serialize`; React cannot receive this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopWorkspaceContextFile {
    /// Opaque owning Workspace identity.
    pub workspace_id: String,
    /// Exact grant revision used for the read.
    pub grant_revision: u64,
    /// Opaque selected entry identity.
    pub entry_id: String,
    /// Bounded presentation-only file label.
    pub display_name: String,
    /// Safe coarse content class.
    pub kind: &'static str,
    /// SHA-256 digest of the exact admitted UTF-8 bytes.
    pub content_digest: String,
    /// Exact bounded UTF-8 content for Runtime, never frontend IPC.
    pub content_utf8: String,
}

/// Path-free aggregate health of durable Workspace authorization recovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopWorkspaceRecoveryStatus {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Stable aggregate state with no private storage details.
    pub state: &'static str,
    /// Number of Workspace grants restored for this process.
    pub restored_count: usize,
    /// Number of grants that require explicit user reauthorization.
    pub needs_reauthorization_count: usize,
}

/// One path-free durable Workspace authorization item for recovery UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopWorkspaceAuthorization {
    /// Exact public schema version.
    pub schema_version: u32,
    /// Stable opaque Workspace identity retained across reauthorization.
    pub workspace_id: String,
    /// Bounded presentation-only root label.
    pub display_name: String,
    /// Monotonic grant revision.
    pub grant_revision: u64,
    /// Whether the Workspace is active or requires explicit reauthorization.
    pub state: &'static str,
}

impl Default for DesktopWorkspaceRecoveryStatus {
    fn default() -> Self {
        Self {
            schema_version: 1,
            state: "ready",
            restored_count: 0,
            needs_reauthorization_count: 0,
        }
    }
}

/// Stable secret- and path-free Workspace failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopWorkspaceError {
    /// Selected root or window ownership is unsafe or invalid.
    CapabilityInvalid,
    /// Selected root cannot be resolved without broadening authority.
    Unavailable,
    /// The bounded active-capability collection is full.
    BoundExceeded,
}

impl DesktopWorkspaceError {
    /// Returns the stable frontend-safe error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::CapabilityInvalid => "workspace_capability_invalid",
            Self::Unavailable => "workspace_unavailable",
            Self::BoundExceeded => "workspace_bound_exceeded",
        }
    }
}

struct PrivateWorkspace {
    public: DesktopWorkspaceGrant,
    owner_window: String,
    canonical_root: PathBuf,
    identity: FileIdentity,
    expires_unix: u64,
    entries: BTreeMap<String, PrivateEntry>,
    #[cfg(target_os = "macos")]
    _security_scope: Option<garive_macos_bookmark::ScopedBookmark>,
}

#[derive(Clone)]
struct PrivateEntry {
    public: DesktopWorkspaceEntry,
    canonical_path: PathBuf,
    identity: FileIdentity,
    is_directory: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    file: u64,
}

/// Backend-only registry for opaque native Workspace selections.
#[derive(Clone, Default)]
pub struct DesktopWorkspaceService {
    active: Arc<Mutex<BTreeMap<String, PrivateWorkspace>>>,
    records: Arc<Mutex<BTreeMap<String, WorkspaceManifestRecord>>>,
    persistence: Option<Arc<WorkspacePersistence>>,
    recovery: Arc<Mutex<DesktopWorkspaceRecoveryStatus>>,
}

struct WorkspacePersistence {
    manifest_path: PathBuf,
    store: Arc<dyn DesktopWorkspaceBookmarkStore>,
}

impl DesktopWorkspaceService {
    /// Constructs a service that persists native bookmark authority privately.
    pub fn durable(manifest_path: PathBuf, store: Arc<dyn DesktopWorkspaceBookmarkStore>) -> Self {
        Self {
            active: Arc::default(),
            records: Arc::default(),
            persistence: Some(Arc::new(WorkspacePersistence {
                manifest_path,
                store,
            })),
            recovery: Arc::default(),
        }
    }

    /// Restores path-free Workspace identities from private native bookmarks.
    pub fn recover(&self, owner_window: &str) -> Result<usize, DesktopWorkspaceError> {
        if owner_window.is_empty() || owner_window.len() > 128 {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let persistence = self
            .persistence
            .as_ref()
            .ok_or(DesktopWorkspaceError::Unavailable)?;
        let records = match read_manifest(&persistence.manifest_path) {
            Ok(records) => records,
            Err(error) => {
                self.set_recovery_status("index_unavailable", 0, 0)?;
                return Err(error);
            }
        };
        let mut record_map = BTreeMap::new();
        let mut needs_reauthorization = 0;
        for record in records {
            if record_map
                .insert(record.workspace_id.clone(), record)
                .is_some()
            {
                needs_reauthorization += 1;
            }
        }
        let now = unix_seconds()?;
        let mut restored = BTreeMap::new();
        for record in record_map.values().cloned() {
            let recovered = (|| {
                let bytes = persistence.store.load(&record.bookmark_ref)?;
                #[cfg(target_os = "macos")]
                let (scope, stale) = garive_macos_bookmark::resolve(&bytes)
                    .map_err(|_| DesktopWorkspaceError::Unavailable)?;
                #[cfg(target_os = "macos")]
                let canonical_root = fs::canonicalize(scope.path())
                    .map_err(|_| DesktopWorkspaceError::Unavailable)?;
                #[cfg(not(target_os = "macos"))]
                let canonical_root = return Err(DesktopWorkspaceError::Unavailable);
                let metadata = fs::symlink_metadata(&canonical_root)
                    .map_err(|_| DesktopWorkspaceError::Unavailable)?;
                let identity = file_identity(&metadata);
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || identity.device != record.device
                    || identity.file != record.file
                {
                    return Err(DesktopWorkspaceError::Unavailable);
                }
                #[cfg(target_os = "macos")]
                if stale {
                    let refreshed = garive_macos_bookmark::create_read_only(&canonical_root)
                        .map_err(|_| DesktopWorkspaceError::Unavailable)?;
                    persistence.store.store(&record.bookmark_ref, &refreshed)?;
                }
                let expires_unix = expires_after(now)?;
                Ok(PrivateWorkspace {
                    public: DesktopWorkspaceGrant {
                        schema_version: 1,
                        workspace_id: record.workspace_id,
                        display_name: record.display_name,
                        access: "enumerate",
                        grant_revision: record.grant_revision,
                        state: "active",
                        expires_at: format_expiry(expires_unix)?,
                    },
                    owner_window: owner_window.into(),
                    canonical_root,
                    identity,
                    expires_unix,
                    entries: BTreeMap::new(),
                    #[cfg(target_os = "macos")]
                    _security_scope: Some(scope),
                })
            })();
            match recovered {
                Ok(workspace) => {
                    restored.insert(workspace.public.workspace_id.clone(), workspace);
                }
                Err(_) => needs_reauthorization += 1,
            }
        }
        let count = restored.len();
        *self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)? = restored;
        *self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)? = record_map;
        self.set_recovery_status(
            if needs_reauthorization == 0 {
                "ready"
            } else {
                "attention_required"
            },
            count,
            needs_reauthorization,
        )?;
        Ok(count)
    }

    /// Returns aggregate durable authorization recovery health without paths.
    pub fn recovery_status(&self) -> Result<DesktopWorkspaceRecoveryStatus, DesktopWorkspaceError> {
        self.recovery
            .lock()
            .map(|status| status.clone())
            .map_err(|_| DesktopWorkspaceError::Unavailable)
    }

    /// Lists bounded path-free durable Workspace authorization states.
    pub fn authorizations(
        &self,
    ) -> Result<Vec<DesktopWorkspaceAuthorization>, DesktopWorkspaceError> {
        let active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let records = self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        Ok(records
            .values()
            .map(|record| DesktopWorkspaceAuthorization {
                schema_version: 1,
                workspace_id: record.workspace_id.clone(),
                display_name: record.display_name.clone(),
                grant_revision: record.grant_revision,
                state: if active.contains_key(&record.workspace_id) {
                    "active"
                } else {
                    "needs_reauthorization"
                },
            })
            .collect())
    }

    /// Reauthorizes one durable Workspace only when the selected root identity matches.
    pub fn reauthorize(
        &self,
        workspace_id: &str,
        selected: &Path,
        owner_window: &str,
    ) -> Result<DesktopWorkspaceGrant, DesktopWorkspaceError> {
        if owner_window.is_empty() || owner_window.len() > 128 {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let persistence = self
            .persistence
            .as_ref()
            .ok_or(DesktopWorkspaceError::Unavailable)?;
        let record = self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?
            .get(workspace_id)
            .cloned()
            .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
        let (canonical_root, metadata) = validated_root(selected)?;
        let identity = file_identity(&metadata);
        if identity.device != record.device || identity.file != record.file {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        #[cfg(target_os = "macos")]
        let bytes = garive_macos_bookmark::create_read_only(&canonical_root)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        #[cfg(not(target_os = "macos"))]
        let bytes: Vec<u8> = return Err(DesktopWorkspaceError::Unavailable);
        persistence.store.store(&record.bookmark_ref, &bytes)?;
        let now = unix_seconds()?;
        let expires_unix = expires_after(now)?;
        let revision = record
            .grant_revision
            .checked_add(1)
            .ok_or(DesktopWorkspaceError::BoundExceeded)?;
        let public = DesktopWorkspaceGrant {
            schema_version: 1,
            workspace_id: workspace_id.to_owned(),
            display_name: display_name(&canonical_root)?,
            access: "enumerate",
            grant_revision: revision,
            state: "active",
            expires_at: format_expiry(expires_unix)?,
        };
        let mut active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let previous = active.insert(
            workspace_id.to_owned(),
            PrivateWorkspace {
                public: public.clone(),
                owner_window: owner_window.into(),
                canonical_root,
                identity,
                expires_unix,
                entries: BTreeMap::new(),
                #[cfg(target_os = "macos")]
                _security_scope: None,
            },
        );
        let mut records = self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let prior_record = records.insert(
            workspace_id.to_owned(),
            WorkspaceManifestRecord {
                grant_revision: revision,
                display_name: public.display_name.clone(),
                ..record
            },
        );
        if let Err(error) = self.persist_records_locked(&records) {
            match previous {
                Some(workspace) => {
                    active.insert(workspace_id.to_owned(), workspace);
                }
                None => {
                    active.remove(workspace_id);
                }
            }
            if let Some(record) = prior_record {
                records.insert(workspace_id.to_owned(), record);
            }
            return Err(error);
        }
        let restored_count = active.len();
        let needs_reauthorization_count = records.len().saturating_sub(restored_count);
        drop(records);
        drop(active);
        self.set_recovery_status(
            if needs_reauthorization_count == 0 {
                "ready"
            } else {
                "attention_required"
            },
            restored_count,
            needs_reauthorization_count,
        )?;
        Ok(public)
    }

    fn set_recovery_status(
        &self,
        state: &'static str,
        restored_count: usize,
        needs_reauthorization_count: usize,
    ) -> Result<(), DesktopWorkspaceError> {
        *self
            .recovery
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)? = DesktopWorkspaceRecoveryStatus {
            schema_version: 1,
            state,
            restored_count,
            needs_reauthorization_count,
        };
        Ok(())
    }

    /// Converts one native picker result into an opaque bounded capability.
    pub fn admit_selected(
        &self,
        selected: &Path,
        owner_window: &str,
    ) -> Result<DesktopWorkspaceGrant, DesktopWorkspaceError> {
        if owner_window.is_empty() || owner_window.len() > 128 {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let (canonical_root, canonical_metadata) = validated_root(selected)?;
        let selected_identity = file_identity(&canonical_metadata);
        let now = unix_seconds()?;
        {
            let mut active = self
                .active
                .lock()
                .map_err(|_| DesktopWorkspaceError::Unavailable)?;
            if let Some(workspace) = active.values_mut().find(|workspace| {
                workspace.owner_window == owner_window && workspace.identity == selected_identity
            }) {
                workspace.expires_unix = expires_after(now)?;
                workspace.public.expires_at = format_expiry(workspace.expires_unix)?;
                return Ok(workspace.public.clone());
            }
        }
        let durable_match = self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?
            .values()
            .find(|record| {
                record.device == selected_identity.device && record.file == selected_identity.file
            })
            .map(|record| record.workspace_id.clone());
        if let Some(workspace_id) = durable_match {
            return self.reauthorize(&workspace_id, selected, owner_window);
        }
        let display_name = display_name(&canonical_root)?;
        let expires_unix = expires_after(now)?;
        let expires_at = format_expiry(expires_unix)?;
        let bookmark_ref = self
            .persistence
            .as_ref()
            .map(|_| format!("bookmark-{}", Uuid::new_v4()));
        if let (Some(persistence), Some(reference)) = (&self.persistence, &bookmark_ref) {
            #[cfg(target_os = "macos")]
            let bytes = garive_macos_bookmark::create_read_only(&canonical_root)
                .map_err(|_| DesktopWorkspaceError::Unavailable)?;
            #[cfg(not(target_os = "macos"))]
            let bytes: Vec<u8> = return Err(DesktopWorkspaceError::Unavailable);
            persistence.store.store(reference, &bytes)?;
        }
        let public = DesktopWorkspaceGrant {
            schema_version: 1,
            workspace_id: format!("workspace-{}", Uuid::new_v4()),
            display_name,
            access: "enumerate",
            grant_revision: 1,
            state: "active",
            expires_at,
        };
        let mut active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        active.retain(|_, value| value.expires_unix >= now);
        if active.len() >= MAX_ACTIVE_WORKSPACES {
            return Err(DesktopWorkspaceError::BoundExceeded);
        }
        active.insert(
            public.workspace_id.clone(),
            PrivateWorkspace {
                public: public.clone(),
                owner_window: owner_window.into(),
                canonical_root,
                identity: selected_identity,
                expires_unix,
                entries: BTreeMap::new(),
                #[cfg(target_os = "macos")]
                _security_scope: None,
            },
        );
        let mut records = self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        if let Some(reference) = bookmark_ref.as_ref() {
            records.insert(
                public.workspace_id.clone(),
                WorkspaceManifestRecord {
                    schema_version: 1,
                    workspace_id: public.workspace_id.clone(),
                    display_name: public.display_name.clone(),
                    grant_revision: public.grant_revision,
                    bookmark_ref: reference.clone(),
                    device: selected_identity.device,
                    file: selected_identity.file,
                },
            );
        }
        if let Err(error) = self.persist_records_locked(&records) {
            active.remove(&public.workspace_id);
            records.remove(&public.workspace_id);
            if let (Some(persistence), Some(reference)) =
                (&self.persistence, bookmark_ref.as_deref())
            {
                let _ = persistence.store.delete(reference);
            }
            return Err(error);
        }
        Ok(public)
    }

    /// Revalidates one exact owner-bound capability without exposing its path.
    pub fn verify(
        &self,
        workspace_id: &str,
        owner_window: &str,
    ) -> Result<DesktopWorkspaceGrant, DesktopWorkspaceError> {
        let now = unix_seconds()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let workspace = active
            .get(workspace_id)
            .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
        if workspace.owner_window != owner_window || workspace.expires_unix < now {
            active.remove(workspace_id);
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let metadata = fs::symlink_metadata(&workspace.canonical_root)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || file_identity(&metadata) != workspace.identity
        {
            return Err(DesktopWorkspaceError::Unavailable);
        }
        Ok(workspace.public.clone())
    }

    /// Revokes one exact capability and drops its private root immediately.
    pub fn revoke(
        &self,
        workspace_id: &str,
        owner_window: &str,
    ) -> Result<(), DesktopWorkspaceError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        if active
            .get(workspace_id)
            .is_none_or(|workspace| workspace.owner_window != owner_window)
        {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let removed = active
            .remove(workspace_id)
            .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
        let mut records = self
            .records
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let removed_record = records.remove(workspace_id);
        if let Err(error) = self.persist_records_locked(&records) {
            active.insert(workspace_id.to_owned(), removed);
            if let Some(record) = removed_record {
                records.insert(workspace_id.to_owned(), record);
            }
            return Err(error);
        }
        if let (Some(persistence), Some(reference)) = (
            &self.persistence,
            removed_record.as_ref().map(|record| &*record.bookmark_ref),
        ) {
            persistence.store.delete(reference)?;
        }
        Ok(())
    }

    /// Lists one explicitly requested directory without exposing a path or file bytes.
    pub fn list_entries(
        &self,
        workspace_id: &str,
        owner_window: &str,
        parent_entry_id: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<DesktopWorkspaceEntryPage, DesktopWorkspaceError> {
        if limit == 0 || limit > MAX_ENTRIES_PER_PAGE {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let offset = parse_cursor(cursor)?;
        let now = unix_seconds()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let invalid = active.get(workspace_id).is_none_or(|workspace| {
            workspace.owner_window != owner_window || workspace.expires_unix < now
        });
        if invalid {
            active.remove(workspace_id);
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let workspace = active
            .get_mut(workspace_id)
            .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
        revalidate_directory(&workspace.canonical_root, workspace.identity)?;
        let directory = match parent_entry_id {
            None => workspace.canonical_root.clone(),
            Some(entry_id) => {
                let parent = workspace
                    .entries
                    .get(entry_id)
                    .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
                if !parent.is_directory || !parent.public.selectable {
                    return Err(DesktopWorkspaceError::CapabilityInvalid);
                }
                revalidate_directory(&parent.canonical_path, parent.identity)?;
                let canonical = fs::canonicalize(&parent.canonical_path)
                    .map_err(|_| DesktopWorkspaceError::Unavailable)?;
                if !canonical.starts_with(&workspace.canonical_root) {
                    return Err(DesktopWorkspaceError::Unavailable);
                }
                canonical
            }
        };
        let mut candidates = Vec::new();
        for result in fs::read_dir(directory).map_err(|_| DesktopWorkspaceError::Unavailable)? {
            if candidates.len() >= MAX_SCANNED_ENTRIES {
                return Err(DesktopWorkspaceError::BoundExceeded);
            }
            let item = result.map_err(|_| DesktopWorkspaceError::Unavailable)?;
            let name = match safe_display_name(&item.file_name()) {
                Ok(name) if !ignored_name(&name) => name,
                _ => continue,
            };
            let metadata = fs::symlink_metadata(item.path())
                .map_err(|_| DesktopWorkspaceError::Unavailable)?;
            if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
                continue;
            }
            let identity = file_identity(&metadata);
            let existing = workspace.entries.values().find(|entry| {
                entry.identity == identity
                    && entry.public.parent_entry_id.as_deref() == parent_entry_id
                    && entry.public.display_name == name
            });
            let entry_id = existing
                .map(|entry| entry.public.entry_id.clone())
                .unwrap_or_else(|| format!("entry-{}", Uuid::new_v4()));
            let is_directory = metadata.is_dir();
            let selectable = !is_directory || !is_package(&name);
            let public = DesktopWorkspaceEntry {
                schema_version: 1,
                entry_id: entry_id.clone(),
                parent_entry_id: parent_entry_id.map(str::to_owned),
                display_name: name.clone(),
                kind: entry_kind(&name, is_directory),
                byte_size: metadata.is_file().then_some(metadata.len()),
                selectable,
            };
            if !workspace.entries.contains_key(&entry_id) {
                if workspace.entries.len() >= MAX_CACHED_ENTRIES {
                    return Err(DesktopWorkspaceError::BoundExceeded);
                }
                workspace.entries.insert(
                    entry_id,
                    PrivateEntry {
                        public: public.clone(),
                        canonical_path: item.path(),
                        identity,
                        is_directory,
                    },
                );
            }
            candidates.push(public);
        }
        candidates.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.entry_id.cmp(&right.entry_id))
        });
        if offset > candidates.len() {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let end = offset.saturating_add(limit).min(candidates.len());
        let has_more = end < candidates.len();
        Ok(DesktopWorkspaceEntryPage {
            schema_version: 1,
            workspace_id: workspace_id.to_owned(),
            parent_entry_id: parent_entry_id.map(str::to_owned),
            entries: candidates[offset..end].to_vec(),
            next_cursor: has_more.then(|| format!("cursor-{end}")),
            has_more,
        })
    }

    /// Reads explicitly selected text entries into a bounded non-serializable value.
    pub fn read_context_files(
        &self,
        workspace_id: &str,
        owner_window: &str,
        entry_ids: &[String],
    ) -> Result<Vec<DesktopWorkspaceContextFile>, DesktopWorkspaceError> {
        if entry_ids.is_empty()
            || entry_ids.len() > MAX_CONTEXT_FILES
            || entry_ids.iter().collect::<BTreeSet<_>>().len() != entry_ids.len()
        {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let now = unix_seconds()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        let invalid = active.get(workspace_id).is_none_or(|workspace| {
            workspace.owner_window != owner_window || workspace.expires_unix < now
        });
        if invalid {
            active.remove(workspace_id);
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let workspace = active
            .get(workspace_id)
            .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
        revalidate_directory(&workspace.canonical_root, workspace.identity)?;
        let mut total_bytes = 0usize;
        let mut context = Vec::with_capacity(entry_ids.len());
        for entry_id in entry_ids {
            let entry = workspace
                .entries
                .get(entry_id)
                .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
            if entry.is_directory || !entry.public.selectable || entry.public.kind != "text" {
                return Err(DesktopWorkspaceError::CapabilityInvalid);
            }
            let before = fs::symlink_metadata(&entry.canonical_path)
                .map_err(|_| DesktopWorkspaceError::Unavailable)?;
            if !before.is_file()
                || before.file_type().is_symlink()
                || file_identity(&before) != entry.identity
                || usize::try_from(before.len()).map_or(true, |size| size > MAX_CONTEXT_FILE_BYTES)
            {
                return Err(DesktopWorkspaceError::Unavailable);
            }
            let bytes =
                fs::read(&entry.canonical_path).map_err(|_| DesktopWorkspaceError::Unavailable)?;
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_CONTEXT_TOTAL_BYTES)
                .ok_or(DesktopWorkspaceError::BoundExceeded)?;
            let after = fs::symlink_metadata(&entry.canonical_path)
                .map_err(|_| DesktopWorkspaceError::Unavailable)?;
            if file_identity(&after) != entry.identity || after.len() != before.len() {
                return Err(DesktopWorkspaceError::Unavailable);
            }
            let content_utf8 =
                String::from_utf8(bytes).map_err(|_| DesktopWorkspaceError::CapabilityInvalid)?;
            context.push(DesktopWorkspaceContextFile {
                workspace_id: workspace_id.to_owned(),
                grant_revision: workspace.public.grant_revision,
                entry_id: entry_id.clone(),
                display_name: entry.public.display_name.clone(),
                kind: entry.public.kind,
                content_digest: hex_digest(content_utf8.as_bytes()),
                content_utf8,
            });
        }
        Ok(context)
    }

    fn persist_records_locked(
        &self,
        records: &BTreeMap<String, WorkspaceManifestRecord>,
    ) -> Result<(), DesktopWorkspaceError> {
        let Some(persistence) = &self.persistence else {
            return Ok(());
        };
        write_manifest(
            &persistence.manifest_path,
            records.values().cloned().collect(),
        )
    }
}

fn expires_after(now: u64) -> Result<u64, DesktopWorkspaceError> {
    now.checked_add(WORKSPACE_LIFETIME_SECONDS)
        .ok_or(DesktopWorkspaceError::Unavailable)
}

fn format_expiry(expires_unix: u64) -> Result<String, DesktopWorkspaceError> {
    DateTime::<Utc>::from_timestamp(
        i64::try_from(expires_unix).map_err(|_| DesktopWorkspaceError::Unavailable)?,
        0,
    )
    .ok_or(DesktopWorkspaceError::Unavailable)
    .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn display_name(root: &Path) -> Result<String, DesktopWorkspaceError> {
    root.file_name()
        .ok_or(DesktopWorkspaceError::CapabilityInvalid)
        .and_then(safe_display_name)
}

fn validated_root(selected: &Path) -> Result<(PathBuf, fs::Metadata), DesktopWorkspaceError> {
    let metadata =
        fs::symlink_metadata(selected).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(DesktopWorkspaceError::CapabilityInvalid);
    }
    let canonical_root =
        fs::canonicalize(selected).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    if canonical_root.parent().is_none() {
        return Err(DesktopWorkspaceError::CapabilityInvalid);
    }
    #[cfg(target_os = "macos")]
    if fs::canonicalize(garive_macos_bookmark::home_directory())
        .is_ok_and(|home| home == canonical_root)
    {
        return Err(DesktopWorkspaceError::CapabilityInvalid);
    }
    let canonical_metadata =
        fs::symlink_metadata(&canonical_root).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    if !canonical_metadata.is_dir() || canonical_metadata.file_type().is_symlink() {
        return Err(DesktopWorkspaceError::CapabilityInvalid);
    }
    Ok((canonical_root, canonical_metadata))
}

fn safe_display_name(value: &std::ffi::OsStr) -> Result<String, DesktopWorkspaceError> {
    let value = value
        .to_str()
        .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
    let filtered = value
        .chars()
        .filter(|character| !character.is_control() && !is_bidi_control(*character))
        .collect::<String>();
    if filtered.is_empty() || filtered.len() > MAX_DISPLAY_NAME_BYTES {
        return Err(DesktopWorkspaceError::CapabilityInvalid);
    }
    Ok(filtered)
}

fn is_bidi_control(character: char) -> bool {
    matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn ignored_name(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target")
}

fn is_package(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".app", ".bundle", ".framework", ".pkg"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn entry_kind(name: &str, is_directory: bool) -> &'static str {
    if is_directory {
        return "directory";
    }
    match Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "md" | "txt" | "json" | "toml" | "yaml" | "yml" | "rs" | "ts" | "tsx" => "text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" => "image",
        "pdf" => "pdf",
        "csv" | "tsv" | "xls" | "xlsx" => "table",
        "ppt" | "pptx" | "key" => "presentation",
        _ => "unknown",
    }
}

fn parse_cursor(cursor: Option<&str>) -> Result<usize, DesktopWorkspaceError> {
    match cursor {
        None => Ok(0),
        Some(value) => value
            .strip_prefix("cursor-")
            .and_then(|offset| offset.parse().ok())
            .ok_or(DesktopWorkspaceError::CapabilityInvalid),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn revalidate_directory(path: &Path, identity: FileIdentity) -> Result<(), DesktopWorkspaceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DesktopWorkspaceError::Unavailable)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || file_identity(&metadata) != identity
    {
        return Err(DesktopWorkspaceError::Unavailable);
    }
    Ok(())
}

fn unix_seconds() -> Result<u64, DesktopWorkspaceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DesktopWorkspaceError::Unavailable)
        .map(|duration| duration.as_secs())
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;
    FileIdentity {
        device: metadata.dev(),
        file: metadata.ino(),
    }
}

#[cfg(not(unix))]
fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: 0,
        file: metadata.len(),
    }
}
