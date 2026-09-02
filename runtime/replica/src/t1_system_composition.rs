//! Host-owned construction of the built-in T1 tool execution surface.

use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};

use garive_core::{AgentToolCapabilities, ToolPreparationPort};
use garive_tools::{
    BuiltinT1Catalogue, PreparationError, PreparedToolCall, ToolIntent, T1_APPLY_PATCH, T1_LIST,
    T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT, T1_WRITE_TEXT,
};
use sha2::{Digest, Sha256};

use crate::{
    BuiltinPatchExecutor, BuiltinProcessExecutor, BuiltinWorkspaceExecutor, ExecutorPort,
    ExecutorRoute, PodmanProcessBackend, PodmanProcessConfig, ProcessIsolationBackend,
    ProcessLaneRegistry, RoutedExecutorPort, T1_PATCH_EXECUTOR_ID, T1_PROCESS_EXECUTOR_ID,
    T1_WORKSPACE_EXECUTOR_ID,
};

/// Stable discriminator for the explicitly configured Podman backend.
pub const PROCESS_BACKEND_PODMAN: &str = "podman";

/// Machine-level Process backend values awaiting one authorized Workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessBackendHostConfig(ProcessBackendHostKind);

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProcessBackendHostKind {
    Podman {
        executable: PathBuf,
        socket_uri: String,
        image: String,
        recovery_root: PathBuf,
        control_timeout_ms: u64,
    },
}

impl ProcessBackendHostConfig {
    /// Constructs an explicit Podman backend without a Workspace or discovery.
    pub fn podman(
        executable: impl Into<PathBuf>,
        socket_uri: impl Into<String>,
        image: impl Into<String>,
        recovery_root: impl Into<PathBuf>,
        control_timeout_ms: u64,
    ) -> Result<Self, String> {
        let executable = executable.into();
        let socket_uri = socket_uri.into();
        let image = image.into();
        let recovery_root = canonical_private_directory(recovery_root.into())?;
        if !executable.is_absolute()
            || !socket_uri.starts_with("unix:///")
            || socket_uri.as_bytes().contains(&0)
            || !digest_pinned_image(&image)
            || control_timeout_ms == 0
            || control_timeout_ms > 30_000
        {
            return Err("invalid Process backend Host configuration".into());
        }
        Ok(Self(ProcessBackendHostKind::Podman {
            executable,
            socket_uri,
            image,
            recovery_root,
            control_timeout_ms,
        }))
    }

    /// Returns the closed backend discriminator.
    pub const fn kind(&self) -> &'static str {
        match self.0 {
            ProcessBackendHostKind::Podman { .. } => PROCESS_BACKEND_PODMAN,
        }
    }

    /// Binds the exact authorized Workspace without changing backend identity.
    pub fn bind_workspace(
        &self,
        workspace_root: impl Into<PathBuf>,
    ) -> Result<ProcessBackendConfig, String> {
        match &self.0 {
            ProcessBackendHostKind::Podman {
                executable,
                socket_uri,
                image,
                recovery_root,
                control_timeout_ms,
            } => ProcessBackendConfig::podman(PodmanProcessConfig::new(
                executable,
                socket_uri,
                image,
                workspace_root,
                recovery_root,
                *control_timeout_ms,
            )?),
        }
    }
}

/// Workspace-bound Process backend used only by Runtime construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessBackendConfig(ProcessBackendKind);

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProcessBackendKind {
    Podman(PodmanProcessConfig),
}

impl ProcessBackendConfig {
    /// Wraps one already validated Podman configuration.
    pub const fn podman(config: PodmanProcessConfig) -> Result<Self, String> {
        Ok(Self(ProcessBackendKind::Podman(config)))
    }

    /// Returns the closed backend discriminator.
    pub const fn kind(&self) -> &'static str {
        match self.0 {
            ProcessBackendKind::Podman(_) => PROCESS_BACKEND_PODMAN,
        }
    }

    fn workspace_root(&self) -> &std::path::Path {
        match &self.0 {
            ProcessBackendKind::Podman(config) => config.workspace_root(),
        }
    }

    fn executor_revision(&self, configured_revision: &str) -> Result<String, String> {
        match &self.0 {
            ProcessBackendKind::Podman(config) => {
                podman_executor_revision(configured_revision, config)
            }
        }
    }

    fn build(&self) -> Arc<dyn ProcessIsolationBackend> {
        match &self.0 {
            ProcessBackendKind::Podman(config) => {
                Arc::new(PodmanProcessBackend::new(config.clone()))
            }
        }
    }
}

impl From<PodmanProcessConfig> for ProcessBackendConfig {
    fn from(config: PodmanProcessConfig) -> Self {
        Self(ProcessBackendKind::Podman(config))
    }
}

/// Persistent machine-level T1 values awaiting one authorized Workspace binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct T1HostSystemConfig {
    policy_revision: String,
    executor_revision: String,
    patch_recovery_root: PathBuf,
    process_lanes: ProcessLaneRegistry,
    process_backend: ProcessBackendHostConfig,
}

impl T1HostSystemConfig {
    /// Validates explicit machine resources without consulting environment or PATH.
    pub fn new(
        policy_revision: impl Into<String>,
        executor_revision: impl Into<String>,
        patch_recovery_root: impl Into<PathBuf>,
        process_lanes: ProcessLaneRegistry,
        process_backend: ProcessBackendHostConfig,
    ) -> Result<Self, String> {
        let policy_revision = policy_revision.into();
        let executor_revision = executor_revision.into();
        let patch_recovery_root = canonical_private_directory(patch_recovery_root.into())?;
        if policy_revision.is_empty() || executor_revision.is_empty() {
            return Err("invalid T1 Host system configuration".into());
        }
        Ok(Self {
            policy_revision,
            executor_revision,
            patch_recovery_root,
            process_lanes,
            process_backend,
        })
    }

    /// Binds one authorized Workspace capability to the persistent host values.
    pub fn bind_workspace(
        &self,
        workspace_root: impl Into<PathBuf>,
    ) -> Result<T1RuntimeSystemConfig, String> {
        let workspace_root = canonical_directory(workspace_root.into())?;
        let process_backend = self.process_backend.bind_workspace(&workspace_root)?;
        T1RuntimeSystemConfig::new(
            self.policy_revision.clone(),
            self.executor_revision.clone(),
            workspace_root,
            self.patch_recovery_root.clone(),
            self.process_lanes.clone(),
            process_backend,
        )
    }

    /// Returns exact configured Process lane identities for Agent resolution.
    pub fn process_lane_names(&self) -> impl Iterator<Item = &str> {
        self.process_lanes.lane_names()
    }

    /// Returns the exact Safety and access policy revision frozen by this Host.
    pub fn policy_revision(&self) -> &str {
        &self.policy_revision
    }

    /// Returns the exact executor revision frozen by this Host.
    pub fn executor_revision(&self) -> &str {
        &self.executor_revision
    }

    /// Resolves the exact snapshot Tool definitions without binding a Workspace.
    pub fn tool_capabilities(&self) -> Result<AgentToolCapabilities, String> {
        let catalogue =
            BuiltinT1Catalogue::new(&self.policy_revision, self.process_lanes.lane_names())
                .map_err(|_| "invalid T1 catalogue")?;
        Ok(AgentToolCapabilities {
            definitions: catalogue.definitions().to_vec(),
        })
    }
}

/// Exact machine-level configuration for one built-in T1 executor set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct T1RuntimeSystemConfig {
    policy_revision: String,
    executor_revision: String,
    workspace_root: PathBuf,
    patch_recovery_root: PathBuf,
    process_lanes: ProcessLaneRegistry,
    process_backend: ProcessBackendConfig,
}

/// Workspace-only T1 configuration for a Runtime without a Process backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct T1WorkspaceRuntimeConfig {
    policy_revision: String,
    executor_revision: String,
    workspace_root: PathBuf,
    patch_recovery_root: PathBuf,
}

impl T1WorkspaceRuntimeConfig {
    /// Validates explicit Workspace and private patch-recovery capabilities.
    pub fn new(
        policy_revision: impl Into<String>,
        executor_revision: impl Into<String>,
        workspace_root: impl Into<PathBuf>,
        patch_recovery_root: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let value = Self {
            policy_revision: policy_revision.into(),
            executor_revision: executor_revision.into(),
            workspace_root: canonical_directory(workspace_root.into())?,
            patch_recovery_root: canonical_private_directory(patch_recovery_root.into())?,
        };
        if value.policy_revision.is_empty() || value.executor_revision.is_empty() {
            return Err("invalid T1 Workspace Runtime configuration".into());
        }
        Ok(value)
    }

    /// Builds read, list, search, create and patch tools without Process authority.
    pub fn build(&self) -> Result<T1RuntimeExecution, String> {
        let catalogue = BuiltinT1Catalogue::new(&self.policy_revision, std::iter::empty::<&str>())
            .map_err(|_| "invalid T1 catalogue")?;
        let workspace = BuiltinWorkspaceExecutor::new(
            &self.workspace_root,
            &self.executor_revision,
            catalogue.clone(),
        )
        .map_err(|_| "T1 workspace executor unavailable")?;
        let patch = BuiltinPatchExecutor::new(
            &self.workspace_root,
            &self.patch_recovery_root,
            &self.executor_revision,
            catalogue.clone(),
        )
        .map_err(|_| "T1 patch executor unavailable")?;
        let executor = RoutedExecutorPort::new([
            ExecutorRoute::new(
                T1_WORKSPACE_EXECUTOR_ID,
                [T1_READ_TEXT, T1_LIST, T1_SEARCH_TEXT, T1_WRITE_TEXT],
                Box::new(workspace),
            )?,
            ExecutorRoute::new(T1_PATCH_EXECUTOR_ID, [T1_APPLY_PATCH], Box::new(patch))?,
        ])?;
        let definitions = catalogue
            .definitions()
            .iter()
            .filter(|definition| definition.name() != T1_PROCESS_RUN)
            .cloned()
            .collect();
        let mut executor_bindings = BTreeMap::new();
        for name in [T1_READ_TEXT, T1_LIST, T1_SEARCH_TEXT, T1_WRITE_TEXT] {
            executor_bindings.insert(
                name.into(),
                T1ExecutorBinding::new(T1_WORKSPACE_EXECUTOR_ID, &self.executor_revision)?,
            );
        }
        executor_bindings.insert(
            T1_APPLY_PATCH.into(),
            T1ExecutorBinding::new(T1_PATCH_EXECUTOR_ID, &self.executor_revision)?,
        );
        Ok(T1RuntimeExecution {
            capabilities: AgentToolCapabilities { definitions },
            preparation: Box::new(T1Preparation(catalogue)),
            executor: Box::new(executor),
            executor_bindings,
        })
    }
}

impl T1RuntimeSystemConfig {
    /// Validates explicit roots, revisions, lanes and Process backend ownership.
    pub fn new(
        policy_revision: impl Into<String>,
        executor_revision: impl Into<String>,
        workspace_root: impl Into<PathBuf>,
        patch_recovery_root: impl Into<PathBuf>,
        process_lanes: ProcessLaneRegistry,
        process_backend: impl Into<ProcessBackendConfig>,
    ) -> Result<Self, String> {
        let policy_revision = policy_revision.into();
        let executor_revision = executor_revision.into();
        let process_backend = process_backend.into();
        let workspace_root = canonical_directory(workspace_root.into())?;
        let patch_recovery_root = canonical_private_directory(patch_recovery_root.into())?;
        if policy_revision.is_empty()
            || executor_revision.is_empty()
            || process_backend.workspace_root() != workspace_root
        {
            return Err("invalid T1 Runtime system configuration".into());
        }
        Ok(Self {
            policy_revision,
            executor_revision,
            workspace_root,
            patch_recovery_root,
            process_lanes,
            process_backend,
        })
    }

    /// Builds the immutable catalogue, preparation port and routed executors.
    pub fn build(&self) -> Result<T1RuntimeExecution, String> {
        let catalogue =
            BuiltinT1Catalogue::new(&self.policy_revision, self.process_lanes.lane_names())
                .map_err(|_| "invalid T1 catalogue")?;
        let workspace = BuiltinWorkspaceExecutor::new(
            &self.workspace_root,
            &self.executor_revision,
            catalogue.clone(),
        )
        .map_err(|_| "T1 workspace executor unavailable")?;
        let patch = BuiltinPatchExecutor::new(
            &self.workspace_root,
            &self.patch_recovery_root,
            &self.executor_revision,
            catalogue.clone(),
        )
        .map_err(|_| "T1 patch executor unavailable")?;
        let process_revision = self
            .process_backend
            .executor_revision(&self.executor_revision)?;
        let backend = self.process_backend.build();
        let process = BuiltinProcessExecutor::new(
            &process_revision,
            catalogue.clone(),
            self.process_lanes.clone(),
            backend,
        )?;
        let executor = RoutedExecutorPort::new([
            ExecutorRoute::new(
                T1_WORKSPACE_EXECUTOR_ID,
                [T1_READ_TEXT, T1_LIST, T1_SEARCH_TEXT, T1_WRITE_TEXT],
                Box::new(workspace),
            )?,
            ExecutorRoute::new(T1_PATCH_EXECUTOR_ID, [T1_APPLY_PATCH], Box::new(patch))?,
            ExecutorRoute::new(T1_PROCESS_EXECUTOR_ID, [T1_PROCESS_RUN], Box::new(process))?,
        ])?;
        let executor_bindings = BTreeMap::from([
            (
                T1_READ_TEXT.into(),
                T1ExecutorBinding::new(T1_WORKSPACE_EXECUTOR_ID, &self.executor_revision)?,
            ),
            (
                T1_LIST.into(),
                T1ExecutorBinding::new(T1_WORKSPACE_EXECUTOR_ID, &self.executor_revision)?,
            ),
            (
                T1_SEARCH_TEXT.into(),
                T1ExecutorBinding::new(T1_WORKSPACE_EXECUTOR_ID, &self.executor_revision)?,
            ),
            (
                T1_WRITE_TEXT.into(),
                T1ExecutorBinding::new(T1_WORKSPACE_EXECUTOR_ID, &self.executor_revision)?,
            ),
            (
                T1_APPLY_PATCH.into(),
                T1ExecutorBinding::new(T1_PATCH_EXECUTOR_ID, &self.executor_revision)?,
            ),
            (
                T1_PROCESS_RUN.into(),
                T1ExecutorBinding::new(T1_PROCESS_EXECUTOR_ID, process_revision)?,
            ),
        ]);
        Ok(T1RuntimeExecution {
            capabilities: AgentToolCapabilities {
                definitions: catalogue.definitions().to_vec(),
            },
            preparation: Box::new(T1Preparation(catalogue)),
            executor: Box::new(executor),
            executor_bindings,
        })
    }
}

/// One Runtime-derived concrete executor identity for an exact T1 Tool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct T1ExecutorBinding {
    executor_id: String,
    executor_revision: String,
}

impl T1ExecutorBinding {
    fn new(
        executor_id: impl Into<String>,
        executor_revision: impl Into<String>,
    ) -> Result<Self, String> {
        let value = Self {
            executor_id: executor_id.into(),
            executor_revision: executor_revision.into(),
        };
        if value.executor_id.is_empty() || value.executor_revision.is_empty() {
            return Err("invalid T1 executor binding".into());
        }
        Ok(value)
    }

    /// Returns the closed Runtime executor route.
    pub fn executor_id(&self) -> &str {
        &self.executor_id
    }

    /// Returns the exact revision, including concrete backend identity.
    pub fn executor_revision(&self) -> &str {
        &self.executor_revision
    }
}

/// Constructed T1 values ready to bind to one effective Agent snapshot.
pub struct T1RuntimeExecution {
    capabilities: AgentToolCapabilities,
    preparation: Box<dyn ToolPreparationPort>,
    executor: Box<dyn ExecutorPort>,
    executor_bindings: BTreeMap<String, T1ExecutorBinding>,
}

impl T1RuntimeExecution {
    /// Returns the exact definitions that an Agent snapshot must contain.
    pub const fn capabilities(&self) -> &AgentToolCapabilities {
        &self.capabilities
    }

    /// Resolves the Runtime-owned concrete executor binding for one exact Tool.
    pub fn executor_binding(&self, tool_name: &str) -> Option<&T1ExecutorBinding> {
        self.executor_bindings.get(tool_name)
    }

    /// Consumes the assembly into Core preparation and Runtime execution ports.
    pub fn into_parts(
        self,
    ) -> (
        AgentToolCapabilities,
        Box<dyn ToolPreparationPort>,
        Box<dyn ExecutorPort>,
    ) {
        (self.capabilities, self.preparation, self.executor)
    }
}

fn podman_executor_revision(
    configured_revision: &str,
    podman: &PodmanProcessConfig,
) -> Result<String, String> {
    let control_timeout = podman.control_timeout_ms().to_string();
    let fields = [
        configured_revision,
        podman
            .podman_executable()
            .to_str()
            .ok_or("Podman executable identity is not UTF-8")?,
        podman.socket_uri(),
        podman.image(),
        podman
            .workspace_root()
            .to_str()
            .ok_or("Podman workspace identity is not UTF-8")?,
        podman
            .recovery_root()
            .to_str()
            .ok_or("Podman recovery identity is not UTF-8")?,
        &control_timeout,
    ];
    let mut digest = Sha256::new();
    digest.update(b"garive.t1.process-executor.podman.v2");
    for field in fields {
        let bytes = field.as_bytes();
        digest.update(
            u64::try_from(bytes.len())
                .map_err(|_| "executor identity too large")?
                .to_be_bytes(),
        );
        digest.update(bytes);
    }
    Ok(format!(
        "{configured_revision}+podman-sha256:{:x}",
        digest.finalize()
    ))
}

struct T1Preparation(BuiltinT1Catalogue);

impl ToolPreparationPort for T1Preparation {
    fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.0.prepare(intent)
    }
}

fn canonical_directory(value: PathBuf) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(value).map_err(|_| "T1 directory unavailable")?;
    if !canonical.is_absolute() || !canonical.is_dir() {
        return Err("T1 directory unavailable".into());
    }
    Ok(canonical)
}

fn canonical_private_directory(value: PathBuf) -> Result<PathBuf, String> {
    let canonical = canonical_directory(value)?;
    let metadata = fs::metadata(&canonical).map_err(|_| "T1 recovery directory unavailable")?;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("T1 recovery directory is not private".into());
    }
    Ok(canonical)
}

fn digest_pinned_image(value: &str) -> bool {
    let Some((name, digest)) = value.rsplit_once("@sha256:") else {
        return false;
    };
    !name.is_empty()
        && !name
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == 0)
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
