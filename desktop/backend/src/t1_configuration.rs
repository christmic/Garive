//! Strict backend-only persistence for machine-level T1 execution resources.

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

use garive_runtime::{
    ProcessBackendHostConfig, ProcessExecutable, ProcessLane, ProcessLaneRegistry,
    T1HostSystemConfig,
};
use serde::Deserialize;

use crate::{
    system_configuration::{unique_json, MAX_DESKTOP_CONFIG_BYTES},
    system_provider::read_bounded,
    DesktopConfigurationError, DesktopSecretResolver,
};

/// Exact versioned machine-tool document name under the app configuration root.
pub const DESKTOP_T1_CONFIG_FILE: &str = "runtime-tools-v1.json";

/// Bounded loader resolving explicit credential references into one Host config.
pub struct DesktopT1ConfigurationProvider<R> {
    document_path: PathBuf,
    app_config_directory: PathBuf,
    secret_resolver: R,
}

impl<R> DesktopT1ConfigurationProvider<R> {
    /// Constructs a provider from exact backend-owned paths and resolver.
    pub fn new(document_path: PathBuf, app_config_directory: PathBuf, secret_resolver: R) -> Self {
        Self {
            document_path,
            app_config_directory,
            secret_resolver,
        }
    }
}

impl<R: DesktopSecretResolver> DesktopT1ConfigurationProvider<R> {
    /// Loads no tool configuration when absent, otherwise one exact Host value.
    pub fn load(&self) -> Result<Option<T1HostSystemConfig>, DesktopConfigurationError> {
        let bytes = match read_bounded(&self.document_path) {
            Ok(value) => value,
            Err(DesktopConfigurationError::NotPresent) => return Ok(None),
            Err(error) => return Err(error),
        };
        if bytes.len() > MAX_DESKTOP_CONFIG_BYTES {
            return Err(DesktopConfigurationError::TooLarge);
        }
        let raw: Document = serde_json::from_value(unique_json(&bytes)?)
            .map_err(|_| DesktopConfigurationError::InvalidDocument)?;
        validate(&raw)?;
        let patch_recovery = private_directory(&self.app_config_directory, &raw.patch_recovery)?;
        let process_recovery =
            private_directory(&self.app_config_directory, &raw.process_recovery)?;
        let mut lanes = Vec::with_capacity(raw.process_lanes.len());
        for lane in raw.process_lanes {
            let executables = lane
                .executables
                .into_iter()
                .map(|item| ProcessExecutable::new(item.alias, item.path))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| DesktopConfigurationError::InvalidValue)?;
            let mut environment = Vec::with_capacity(lane.environment.len());
            for (key, source) in lane.environment {
                let value = match source {
                    EnvironmentSource::Literal(source) => source.literal,
                    EnvironmentSource::Credential(source) => self
                        .secret_resolver
                        .resolve(&source.credential_ref)?
                        .expose_secret()
                        .to_owned(),
                };
                environment.push((key, value));
            }
            lanes.push(
                ProcessLane::new(lane.name, executables, environment)
                    .map_err(|_| DesktopConfigurationError::InvalidValue)?,
            );
        }
        let lanes =
            ProcessLaneRegistry::new(lanes).map_err(|_| DesktopConfigurationError::InvalidValue)?;
        let process_backend = ProcessBackendHostConfig::podman(
            raw.podman.executable,
            raw.podman.socket_uri,
            raw.podman.image,
            process_recovery,
            raw.podman.control_timeout_ms,
        )
        .map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
        T1HostSystemConfig::new(
            raw.policy_revision,
            raw.executor_revision,
            patch_recovery,
            lanes,
            process_backend,
        )
        .map(Some)
        .map_err(|_| DesktopConfigurationError::ConstructionFailure)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    policy_revision: String,
    executor_revision: String,
    patch_recovery: String,
    process_recovery: String,
    podman: PodmanDocument,
    process_lanes: Vec<ProcessLaneDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PodmanDocument {
    executable: PathBuf,
    socket_uri: String,
    image: String,
    control_timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessLaneDocument {
    name: String,
    executables: Vec<ExecutableDocument>,
    #[serde(default)]
    environment: BTreeMap<String, EnvironmentSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutableDocument {
    alias: String,
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EnvironmentSource {
    Literal(LiteralEnvironmentSource),
    Credential(CredentialEnvironmentSource),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LiteralEnvironmentSource {
    literal: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialEnvironmentSource {
    credential_ref: String,
}

fn validate(raw: &Document) -> Result<(), DesktopConfigurationError> {
    if raw.schema_version != 1
        || raw.policy_revision.is_empty()
        || raw.executor_revision.is_empty()
        || raw.process_lanes.is_empty()
        || raw.process_lanes.len() > 16
        || !single_component(&raw.patch_recovery)
        || !single_component(&raw.process_recovery)
        || raw.patch_recovery == raw.process_recovery
        || raw.process_lanes.iter().any(|lane| {
            lane.executables.is_empty()
                || lane.executables.len() > 32
                || lane.environment.len() > 64
                || lane.environment.values().any(|source| match source {
                    EnvironmentSource::Literal(source) => source.literal.len() > 16_384,
                    EnvironmentSource::Credential(source) => {
                        let credential_ref = &source.credential_ref;
                        credential_ref.is_empty() || credential_ref.len() > 256
                    }
                })
        })
    {
        return Err(DesktopConfigurationError::InvalidValue);
    }
    Ok(())
}

fn single_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn private_directory(root: &Path, name: &str) -> Result<PathBuf, DesktopConfigurationError> {
    let target = root.join(name);
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(&target) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(DesktopConfigurationError::ReadFailure),
    }
    let metadata =
        fs::symlink_metadata(&target).map_err(|_| DesktopConfigurationError::ReadFailure)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(DesktopConfigurationError::InvalidValue);
    }
    fs::canonicalize(target).map_err(|_| DesktopConfigurationError::ReadFailure)
}
