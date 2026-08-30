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
    /// Public lifecycle state.
    pub state: &'static str,
    /// Canonical UTC expiry instant.
    pub expires_at: String,
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
}

fn display_name(root: &Path) -> Result<String, DesktopWorkspaceError> {
    let value = root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(DesktopWorkspaceError::CapabilityInvalid)?;
    let filtered = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if filtered.is_empty() || filtered.len() > MAX_DISPLAY_NAME_BYTES {
        return Err(DesktopWorkspaceError::CapabilityInvalid);
    }
    Ok(filtered)
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
