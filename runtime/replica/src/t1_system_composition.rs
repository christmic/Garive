//! Host-owned construction of the built-in T1 tool execution surface.

use std::{fs, path::PathBuf, sync::Arc};

use garive_core::{AgentToolCapabilities, ToolPreparationPort};
use garive_tools::{
    BuiltinT1Catalogue, PreparationError, PreparedToolCall, ToolIntent, T1_APPLY_PATCH, T1_LIST,
    T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT,
};

use crate::{
    BuiltinPatchExecutor, BuiltinProcessExecutor, BuiltinWorkspaceExecutor, ExecutorPort,
    ExecutorRoute, PodmanProcessBackend, PodmanProcessConfig, ProcessIsolationBackend,
    ProcessLaneRegistry, RoutedExecutorPort, T1_PATCH_EXECUTOR_ID, T1_PROCESS_EXECUTOR_ID,
    T1_WORKSPACE_EXECUTOR_ID,
};

/// Exact machine-level configuration for one built-in T1 executor set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct T1RuntimeSystemConfig {
    policy_revision: String,
    executor_revision: String,
    workspace_root: PathBuf,
    patch_recovery_root: PathBuf,
    process_lanes: ProcessLaneRegistry,
    podman: PodmanProcessConfig,
}

impl T1RuntimeSystemConfig {
    /// Validates explicit roots, revisions, lanes and Podman ownership.
    pub fn new(
        policy_revision: impl Into<String>,
        executor_revision: impl Into<String>,
        workspace_root: impl Into<PathBuf>,
        patch_recovery_root: impl Into<PathBuf>,
        process_lanes: ProcessLaneRegistry,
        podman: PodmanProcessConfig,
    ) -> Result<Self, String> {
        let policy_revision = policy_revision.into();
        let executor_revision = executor_revision.into();
        let workspace_root = canonical_directory(workspace_root.into())?;
        let patch_recovery_root = canonical_private_directory(patch_recovery_root.into())?;
        if policy_revision.is_empty()
            || executor_revision.is_empty()
            || podman.workspace_root() != workspace_root
        {
            return Err("invalid T1 Runtime system configuration".into());
        }
        Ok(Self {
            policy_revision,
            executor_revision,
            workspace_root,
            patch_recovery_root,
            process_lanes,
            podman,
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
        let backend: Arc<dyn ProcessIsolationBackend> =
            Arc::new(PodmanProcessBackend::new(self.podman.clone()));
        let process = BuiltinProcessExecutor::new(
            &self.executor_revision,
            catalogue.clone(),
            self.process_lanes.clone(),
            backend,
        )?;
        let executor = RoutedExecutorPort::new([
            ExecutorRoute::new(
                T1_WORKSPACE_EXECUTOR_ID,
                [T1_READ_TEXT, T1_LIST, T1_SEARCH_TEXT],
                Box::new(workspace),
            )?,
            ExecutorRoute::new(T1_PATCH_EXECUTOR_ID, [T1_APPLY_PATCH], Box::new(patch))?,
            ExecutorRoute::new(T1_PROCESS_EXECUTOR_ID, [T1_PROCESS_RUN], Box::new(process))?,
        ])?;
        Ok(T1RuntimeExecution {
            capabilities: AgentToolCapabilities {
                definitions: catalogue.definitions().to_vec(),
            },
            preparation: Box::new(T1Preparation(catalogue)),
            executor: Box::new(executor),
        })
    }
}

/// Constructed T1 values ready to bind to one effective Agent snapshot.
pub struct T1RuntimeExecution {
    capabilities: AgentToolCapabilities,
    preparation: Box<dyn ToolPreparationPort>,
    executor: Box<dyn ExecutorPort>,
}

impl T1RuntimeExecution {
    /// Returns the exact definitions that an Agent snapshot must contain.
    pub const fn capabilities(&self) -> &AgentToolCapabilities {
        &self.capabilities
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
