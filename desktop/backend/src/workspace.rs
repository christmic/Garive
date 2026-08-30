use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use uuid::Uuid;

const MAX_ACTIVE_WORKSPACES: usize = 16;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_ENTRIES_PER_PAGE: usize = 64;
const MAX_SCANNED_ENTRIES: usize = 512;
const MAX_CACHED_ENTRIES: usize = 4_096;
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
#[derive(Default)]
pub struct DesktopWorkspaceService {
    active: Mutex<BTreeMap<String, PrivateWorkspace>>,
}

impl DesktopWorkspaceService {
    /// Converts one native picker result into an opaque bounded capability.
    pub fn admit_selected(
        &self,
        selected: &Path,
        owner_window: &str,
    ) -> Result<DesktopWorkspaceGrant, DesktopWorkspaceError> {
        if owner_window.is_empty() || owner_window.len() > 128 {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
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
        let canonical_metadata = fs::symlink_metadata(&canonical_root)
            .map_err(|_| DesktopWorkspaceError::Unavailable)?;
        if !canonical_metadata.is_dir() || canonical_metadata.file_type().is_symlink() {
            return Err(DesktopWorkspaceError::CapabilityInvalid);
        }
        let display_name = display_name(&canonical_root)?;
        let now = unix_seconds()?;
        let expires_unix = now
            .checked_add(WORKSPACE_LIFETIME_SECONDS)
            .ok_or(DesktopWorkspaceError::Unavailable)?;
        let expires_at = DateTime::<Utc>::from_timestamp(
            i64::try_from(expires_unix).map_err(|_| DesktopWorkspaceError::Unavailable)?,
            0,
        )
        .ok_or(DesktopWorkspaceError::Unavailable)?
        .to_rfc3339_opts(SecondsFormat::Secs, true);
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
                identity: file_identity(&canonical_metadata),
                expires_unix,
                entries: BTreeMap::new(),
            },
        );
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
        active.remove(workspace_id);
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
}

fn display_name(root: &Path) -> Result<String, DesktopWorkspaceError> {
    root.file_name()
        .ok_or(DesktopWorkspaceError::CapabilityInvalid)
        .and_then(safe_display_name)
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
