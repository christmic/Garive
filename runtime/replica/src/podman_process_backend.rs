//! Podman-owned process-tree isolation configured entirely by Runtime.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use garive_tools::ToolInvocationId;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    podman_process_artifact::EnvironmentArtifact,
    podman_process_cli::{AttachCompletion, PodmanCli},
    ProcessBackendError, ProcessExecutionRequest, ProcessExecutionResult, ProcessExit,
    ProcessIsolationBackend, ProcessWorkspaceMode,
};

const CONTAINER_WORKSPACE: &str = "/workspace";
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

/// Concrete process boundary backed by an explicitly selected Podman service.
pub struct PodmanProcessBackend {
    config: PodmanProcessConfig,
}

impl PodmanProcessBackend {
    /// Constructs the backend without consulting environment or Podman defaults.
    pub const fn new(config: PodmanProcessConfig) -> Self {
        Self { config }
    }

    fn cli(&self) -> PodmanCli<'_> {
        PodmanCli::new(&self.config.podman_executable, &self.config.socket_uri)
    }

    fn name(&self, invocation: &ToolInvocationId, attempt: &str) -> String {
        let digest = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{attempt}", invocation.as_str()).as_bytes())
        );
        format!("garive-process-{}", &digest[..32])
    }

    fn exists(&self, name: &str) -> Result<bool, ProcessBackendError> {
        let output = self
            .cli()
            .output(&["container".into(), "exists".into(), name.into()])
            .map_err(|()| ProcessBackendError::StateUnknown)?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(ProcessBackendError::StateUnknown),
        }
    }

    fn inspect(&self, name: &str) -> Result<ContainerState, ProcessBackendError> {
        self.prove_ownership(name)?;
        let output = self
            .cli()
            .output(&[
                "inspect".into(),
                "--format".into(),
                "{{json .State}}".into(),
                name.into(),
            ])
            .map_err(|()| ProcessBackendError::StateUnknown)?;
        if !output.status.success() || output.truncated || !output.stderr.is_empty() {
            return Err(ProcessBackendError::StateUnknown);
        }
        let state: ContainerState = serde_json::from_slice(&output.stdout)
            .map_err(|_| ProcessBackendError::StateUnknown)?;
        if state.error.is_empty() {
            Ok(state)
        } else {
            Err(ProcessBackendError::StateUnknown)
        }
    }

    fn prove_ownership(&self, name: &str) -> Result<(), ProcessBackendError> {
        let output = self
            .cli()
            .output(&[
                "inspect".into(),
                "--format".into(),
                "{{index .Config.Labels \"io.garive.runtime.owner\"}}".into(),
                name.into(),
            ])
            .map_err(|()| ProcessBackendError::StateUnknown)?;
        if output.status.success()
            && !output.truncated
            && output.stderr.is_empty()
            && String::from_utf8(output.stdout).is_ok_and(|value| value.trim() == name)
        {
            Ok(())
        } else {
            Err(ProcessBackendError::StateUnknown)
        }
    }

    fn kill_and_prove(&self, name: &str) -> Result<(), ProcessBackendError> {
        if !self.exists(name)? {
            return Ok(());
        }
        let state = self.inspect(name)?;
        if state.running {
            let output = self
                .cli()
                .output(&["kill".into(), "--signal".into(), "KILL".into(), name.into()])
                .map_err(|()| ProcessBackendError::StateUnknown)?;
            if !output.status.success() {
                return Err(ProcessBackendError::StateUnknown);
            }
        }
        let state = self.inspect(name)?;
        if state.running || state.pid != 0 {
            return Err(ProcessBackendError::StateUnknown);
        }
        Ok(())
    }

    fn remove(&self, name: &str) -> Result<(), ProcessBackendError> {
        if self.exists(name)? {
            self.prove_ownership(name)?;
            let output = self
                .cli()
                .output(&["rm".into(), name.into()])
                .map_err(|()| ProcessBackendError::StateUnknown)?;
            if !output.status.success() {
                return Err(ProcessBackendError::StateUnknown);
            }
        }
        if self.exists(name)? {
            return Err(ProcessBackendError::StateUnknown);
        }
        Ok(())
    }
}

impl ProcessIsolationBackend for PodmanProcessBackend {
    fn preflight(&self, request: &ProcessExecutionRequest) -> Result<(), String> {
        validate_request(request)
    }

    fn execute(
        &self,
        request: ProcessExecutionRequest,
    ) -> Result<ProcessExecutionResult, ProcessBackendError> {
        validate_request(&request).map_err(|_| ProcessBackendError::Unavailable)?;
        let name = self.name(&request.invocation_id, &request.dispatch_attempt_id);
        if self.exists(&name)? {
            return Err(ProcessBackendError::StateUnknown);
        }
        let artifact =
            EnvironmentArtifact::create(&self.config.recovery_root, &name, &request.environment)
                .map_err(|()| ProcessBackendError::StateUnknown)?;
        let create = create_arguments(&self.config, &request, &name, artifact.path())?;
        let created = self
            .cli()
            .output(&create)
            .map_err(|()| ProcessBackendError::StateUnknown)?;
        artifact
            .remove()
            .map_err(|()| ProcessBackendError::StateUnknown)?;
        if !created.status.success() {
            return if self.exists(&name)? {
                Err(ProcessBackendError::StateUnknown)
            } else {
                Err(ProcessBackendError::Unavailable)
            };
        }
        let start = ["start".into(), "--attach".into(), name.clone()];
        let completion = self.cli().attach(
            &start,
            usize::try_from(request.max_output_bytes).unwrap_or(usize::MAX),
            Duration::from_millis(request.timeout_ms),
            || self.kill_and_prove(&name).map_err(|_| ()),
        );
        let completion = completion.map_err(|()| ProcessBackendError::StateUnknown)?;
        let state = self.inspect(&name)?;
        if state.running || state.pid != 0 {
            return Err(ProcessBackendError::StateUnknown);
        }
        let (output, exit) = match completion {
            AttachCompletion::Exited(output) => (output, ProcessExit::Code(state.exit_code)),
            AttachCompletion::TimedOut(output) => (output, ProcessExit::Timeout),
        };
        Ok(ProcessExecutionResult {
            exit,
            stdout: output.stdout,
            stderr: output.stderr,
            truncated: output.truncated,
            process_tree_terminated: true,
        })
    }

    fn acknowledge_terminal(
        &self,
        invocation_id: &ToolInvocationId,
        dispatch_attempt_id: &str,
    ) -> Result<(), ProcessBackendError> {
        self.remove(&self.name(invocation_id, dispatch_attempt_id))
    }

    fn terminate_or_prove_absent(
        &self,
        invocation_id: &ToolInvocationId,
        dispatch_attempt_id: &str,
    ) -> Result<(), ProcessBackendError> {
        let name = self.name(invocation_id, dispatch_attempt_id);
        EnvironmentArtifact::remove_if_present(&self.config.recovery_root, &name)
            .map_err(|()| ProcessBackendError::StateUnknown)?;
        self.kill_and_prove(&name)?;
        self.remove(&name)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerState {
    running: bool,
    pid: i64,
    exit_code: i32,
    #[serde(default)]
    error: String,
}

fn validate_request(request: &ProcessExecutionRequest) -> Result<(), String> {
    if request.argv.is_empty()
        || request.argv.len() > 256
        || !request.executable.is_absolute()
        || request.max_output_bytes == 0
        || request.max_output_bytes > 1_048_576
        || request.timeout_ms == 0
        || request.timeout_ms > 300_000
        || request.max_processes == 0
        || request.max_open_files == 0
        || request
            .argv
            .iter()
            .any(|value| value.is_empty() || has_nul(value))
        || request.environment.iter().any(|(key, value)| {
            !valid_environment_key(key) || has_nul(value) || value.contains(['\n', '\r'])
        })
    {
        return Err("invalid Podman process request".into());
    }
    container_working_directory(&request.working_directory).map(|_| ())
}

fn create_arguments(
    config: &PodmanProcessConfig,
    request: &ProcessExecutionRequest,
    name: &str,
    environment_file: &Path,
) -> Result<Vec<String>, ProcessBackendError> {
    let mode = match request.workspace_mode {
        ProcessWorkspaceMode::Read => "true",
        ProcessWorkspaceMode::Write => "false",
    };
    let mount = format!(
        "type=bind,source={},destination={CONTAINER_WORKSPACE},ro={mode}",
        config.workspace_root.display()
    );
    let mut values = vec![
        "create".into(),
        "--name".into(),
        name.into(),
        "--label".into(),
        format!("io.garive.runtime.owner={name}"),
        "--pull=never".into(),
        "--network=none".into(),
        "--read-only".into(),
        "--cap-drop=all".into(),
        "--security-opt=no-new-privileges".into(),
        "--pids-limit".into(),
        request.max_processes.to_string(),
        "--ulimit".into(),
        format!("nofile={0}:{0}", request.max_open_files),
        "--workdir".into(),
        container_working_directory(&request.working_directory)
            .map_err(|_| ProcessBackendError::Unavailable)?,
        "--mount".into(),
        mount,
        "--tmpfs".into(),
        "/tmp:rw,nosuid,nodev,noexec,size=67108864".into(),
        "--env-file".into(),
        environment_file
            .to_str()
            .ok_or(ProcessBackendError::Unavailable)?
            .into(),
        config.image.clone(),
        request
            .executable
            .to_str()
            .ok_or(ProcessBackendError::Unavailable)?
            .into(),
    ];
    values.extend(request.argv.iter().skip(1).cloned());
    Ok(values)
}

fn container_working_directory(value: &str) -> Result<String, String> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err("invalid process working directory".into());
    }
    Ok(if value == "." {
        CONTAINER_WORKSPACE.into()
    } else {
        format!("{CONTAINER_WORKSPACE}/{value}")
    })
}

fn valid_environment_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

fn has_nul(value: &str) -> bool {
    value.as_bytes().contains(&0)
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
            || value
                .workspace_root
                .to_str()
                .is_none_or(|path| path.contains(','))
            || value.recovery_root.to_str().is_none()
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
