//! Podman-owned process-tree isolation configured entirely by Runtime.

use std::{
    fs,
    path::{Path, PathBuf},
};

const IMAGE_DIGEST_PREFIX: &str = "sha256:";

/// Immutable Podman boundary supplied explicitly by Runtime construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PodmanProcessConfig {
    podman_executable: PathBuf,
    socket_uri: String,
    image: String,
    workspace_root: PathBuf,
    recovery_root: PathBuf,
}

impl PodmanProcessConfig {
    /// Validates exact executable, socket, image and private filesystem roots.
    pub fn new(
        podman_executable: impl Into<PathBuf>,
        socket_uri: impl Into<String>,
        image: impl Into<String>,
        workspace_root: impl Into<PathBuf>,
        recovery_root: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let value = Self {
            podman_executable: podman_executable.into(),
            socket_uri: socket_uri.into(),
            image: image.into(),
            workspace_root: canonical_directory(workspace_root.into())?,
            recovery_root: canonical_private_directory(recovery_root.into())?,
        };
        if !value.podman_executable.is_absolute()
            || !value.socket_uri.starts_with("unix:///")
            || value.socket_uri.as_bytes().contains(&0)
            || !digest_pinned_image(&value.image)
            || value.workspace_root.to_string_lossy().contains(',')
        {
            return Err("invalid Podman process configuration".into());
        }
        Ok(value)
    }

    /// Returns the exact Podman executable without PATH discovery.
    pub fn podman_executable(&self) -> &Path {
        &self.podman_executable
    }

    /// Returns the explicit Podman service socket URI.
    pub fn socket_uri(&self) -> &str {
        &self.socket_uri
    }

    /// Returns the immutable digest-pinned execution image.
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Returns the canonical host workspace root.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the canonical Runtime-private recovery root.
    pub fn recovery_root(&self) -> &Path {
        &self.recovery_root
    }
}

fn canonical_directory(value: PathBuf) -> Result<PathBuf, String> {
    if !value.is_absolute() {
        return Err("process directory must be absolute".into());
    }
    let canonical = fs::canonicalize(value).map_err(|_| "process directory is unavailable")?;
    if !canonical.is_dir() {
        return Err("process directory is unavailable".into());
    }
    Ok(canonical)
}

#[cfg(unix)]
fn canonical_private_directory(value: PathBuf) -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(&value)
        .map_err(|_| "process recovery directory is unavailable")?;
    let canonical = canonical_directory(value)?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|_| "process recovery directory is unavailable")?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::getuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("process recovery directory is not private".into());
    }
    Ok(canonical)
}

fn digest_pinned_image(value: &str) -> bool {
    let Some((name, digest)) = value.rsplit_once('@') else {
        return false;
    };
    let Some(hex) = digest.strip_prefix(IMAGE_DIGEST_PREFIX) else {
        return false;
    };
    !name.is_empty() && hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    const IMAGE: &str = "docker.io/library/alpine@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b";

    #[test]
    fn configuration_is_explicit_private_and_digest_pinned() {
        let temporary = tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let recovery = temporary.path().join("recovery");
        let config = PodmanProcessConfig::new(
            "/opt/podman",
            "unix:///private/tmp/podman.sock",
            IMAGE,
            &workspace,
            &recovery,
        )
        .unwrap();
        assert_eq!(config.podman_executable(), Path::new("/opt/podman"));
        assert_eq!(
            config.workspace_root(),
            fs::canonicalize(workspace).unwrap()
        );
        assert!(PodmanProcessConfig::new(
            "/opt/podman",
            "unix:///private/tmp/podman.sock",
            "docker.io/library/alpine:latest",
            temporary.path(),
            recovery,
        )
        .is_err());
    }
}
