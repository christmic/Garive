//! Embedded local Runtime composition behind typed Desktop IPC values.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::{path::PathBuf, sync::Arc};

use garive_llm::ModelPort;
use garive_runtime::{
    local_dispatch_queue, HostClock, HostContinuationInput, InstalledAgent, LiveHost,
    LiveHostLimits, LocalDispatchQueue, LocalExecutionAttempt, LocalExecutionPolicy,
    LocalExecutionWorker, TurnCommandResponse,
};
use serde::Serialize;
use tokio::sync::Mutex;

mod setup;
mod system_configuration;
mod system_provider;
mod workspace;

/// Durable path-free Workspace attachment exposed to Desktop clients.
pub use garive_runtime::HostWorkspaceAttachment as DesktopWorkspaceAttachment;
/// Restart-safe durable Session summary exposed to Desktop clients.
pub use garive_runtime::SessionSummary as DesktopSessionSummary;
/// Restart-safe durable Turn timeline exposed to Desktop clients.
pub use garive_runtime::TurnTimelinePage as DesktopTimelinePage;
pub use setup::{
    DesktopSetupCancellation, DesktopSetupCatalogue, DesktopSetupError, DesktopSetupInput,
    DesktopSetupPlan, DesktopSetupProfile, DesktopSetupReceipt, DesktopSetupService,
    DesktopSetupSummary, SetupClock, SetupCredentialStore, SystemSetupClock,
    SystemSetupCredentialStore,
};
pub use system_configuration::{
    DesktopConfigurationError, DesktopSystemConfiguration, MAX_DESKTOP_CONFIG_BYTES,
};
pub use system_provider::{
    BuiltinDesktopProfileRegistry, DesktopConfigurationProvider, DesktopProfileConfiguration,
    DesktopProfileRegistry, DesktopSecretResolver, FileDesktopConfigurationProvider,
    SystemDesktopSecretResolver, ANTHROPIC_MESSAGES_PROFILE_ID, DESKTOP_CONFIG_FILE,
    DESKTOP_CREDENTIAL_SERVICE, OPENAI_RESPONSES_PROFILE_ID,
};
pub use workspace::{
    DesktopWorkspaceContextFile, DesktopWorkspaceEntry, DesktopWorkspaceEntryPage,
    DesktopWorkspaceError, DesktopWorkspaceGrant, DesktopWorkspaceService,
};

/// Explicit operational identities and clock values owned by Desktop backend composition.
pub trait DesktopOperations: Send + Sync {
    /// Creates one stable printable idempotency identity for a named command.
    fn command_id(&self, purpose: &'static str) -> Result<String, DesktopHostError>;
    /// Creates one explicit lease/clock attempt for a committed Execution.
    fn execution_attempt(&self) -> Result<LocalExecutionAttempt, DesktopHostError>;
}

/// Complete constructed values needed by one embedded Desktop Runtime.
pub struct DesktopHostConfig {
    /// Durable Garive SQLite path selected by the backend configuration layer.
    pub database_path: PathBuf,
    /// Installed immutable Agent definition.
    pub installed_agent: InstalledAgent,
    /// Bounded Host command and projection policy.
    pub host_limits: LiveHostLimits,
    /// Bounded local Agent execution policy.
    pub execution_policy: LocalExecutionPolicy,
    /// Non-zero committed-Turn queue capacity.
    pub dispatch_capacity: usize,
    /// Backend-owned durable Host clock.
    pub host_clock: Arc<dyn HostClock>,
    /// Fully constructed Provider-neutral model port.
    pub model: Arc<dyn ModelPort>,
    /// Backend-owned command, lease and execution clock source.
    pub operations: Arc<dyn DesktopOperations>,
}

/// Typed durable terminal returned to the Desktop frontend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTerminal {
    /// Turn completed with committed text.
    Completed,
    /// Turn suspended and requires a later continuation.
    Suspended,
    /// Turn stopped durably.
    Stopped,
    /// Turn failed durably.
    Failed,
}

/// Typed IPC projection of one embedded Runtime Turn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopTurnResult {
    /// Durable Session identity.
    pub session_id: String,
    /// Durable Turn identity.
    pub turn_id: String,
    /// Disposable Execution identity.
    pub execution_id: String,
    /// Highest scanned durable position.
    pub cursor: u64,
    /// Explicit durable terminal.
    pub terminal: DesktopTerminal,
    /// Committed completion text, empty for other terminals.
    pub text: String,
}

/// Truthful capability snapshot for the currently installed Desktop backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopCapabilityManifest {
    /// Whether an embedded Runtime composition is installed and ready.
    pub configured: bool,
    /// Public installed Agent definition accepted for new Sessions.
    pub agent_definition_id: Option<String>,
    /// Whether the current process can continue a known durable Session.
    pub multi_turn: bool,
    /// Whether H2 durable Session discovery and timeline reload are installed.
    pub durable_navigation: bool,
    /// Whether H3 committed Agent activity projection is installed.
    pub activity: bool,
    /// Whether A-DESKTOP-C2 write-only setup is installed.
    pub setup: bool,
    /// Whether opaque local Workspace capabilities are installed.
    pub workspaces: bool,
    /// Whether bounded artifact projection and preview are installed.
    pub artifacts: bool,
}

/// Stable secret-free embedded Desktop Host failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopHostError {
    /// Garive system configuration has not installed a backend composition.
    NotConfigured,
    /// Explicit composition values are invalid.
    InvalidConfiguration,
    /// A Host command failed validation or durability.
    HostFailure,
    /// The local worker did not commit a terminal.
    ExecutionFailure,
    /// Durable events did not contain one exact terminal for the Turn.
    ProjectionFailure,
}

impl DesktopHostError {
    /// Returns the stable frontend-safe error name.
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::InvalidConfiguration => "invalid_configuration",
            Self::HostFailure => "host_failure",
            Self::ExecutionFailure => "execution_failure",
            Self::ProjectionFailure => "projection_failure",
        }
    }
}

/// One embedded R1 composition used behind Tauri state.
pub struct DesktopHost {
    host: LiveHost,
    definition_id: String,
    worker: LocalExecutionWorker,
    queue: Mutex<LocalDispatchQueue>,
    operations: Arc<dyn DesktopOperations>,
}

impl DesktopHost {
    /// Constructs R1 entirely from Garive backend values without discovery.
    pub fn new(config: DesktopHostConfig) -> Result<Self, DesktopHostError> {
        if config.database_path.as_os_str().is_empty() || config.dispatch_capacity == 0 {
            return Err(DesktopHostError::InvalidConfiguration);
        }
        let definition_id = config.installed_agent.definition_id.clone();
        let (dispatcher, queue) = local_dispatch_queue(config.dispatch_capacity)
            .map_err(|_| DesktopHostError::InvalidConfiguration)?;
        let host = LiveHost::new(
            &config.database_path,
            config.installed_agent,
            config.host_limits,
            config.host_clock,
            dispatcher,
        )
        .map_err(|_| DesktopHostError::InvalidConfiguration)?;
        let worker =
            LocalExecutionWorker::new(&config.database_path, config.execution_policy, config.model)
                .map_err(|_| DesktopHostError::InvalidConfiguration)?;
        Ok(Self {
            host,
            definition_id,
            worker,
            queue: Mutex::new(queue),
            operations: config.operations,
        })
    }

    fn supports_activity(&self) -> bool {
        self.host.limits().activity.is_some()
    }

    /// Creates one empty durable Session before attaching selected context.
    pub fn create_session(&self, definition_id: &str) -> Result<String, DesktopHostError> {
        let command_id = self.operations.command_id("create")?;
        self.host
            .create_session(&command_id, definition_id)
            .map(|response| response.session_id)
            .map_err(|_| DesktopHostError::HostFailure)
    }

    /// Commits one verified opaque Workspace attachment before a Turn starts.
    pub fn attach_workspace(
        &self,
        session_id: &str,
        workspace: &DesktopWorkspaceGrant,
    ) -> Result<DesktopWorkspaceAttachment, DesktopHostError> {
        let command_id = self.operations.command_id("attach-workspace")?;
        self.host
            .attach_workspace(
                &command_id,
                session_id,
                &workspace.workspace_id,
                &workspace.display_name,
                workspace.grant_revision,
                workspace.access,
            )
            .map_err(|_| DesktopHostError::HostFailure)
    }

    /// Restores the latest durable opaque Workspace attachments for one Session.
    pub fn session_workspaces(
        &self,
        session_id: &str,
    ) -> Result<Vec<DesktopWorkspaceAttachment>, DesktopHostError> {
        self.host
            .session_workspaces(session_id)
            .map_err(|_| DesktopHostError::HostFailure)
    }

    /// Creates a Session and executes one Turn through the embedded durable loop.
    pub async fn run_turn(
        &self,
        definition_id: &str,
        input: &str,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        self.run_turn_in_session(definition_id, None, input).await
    }

    /// Starts one Turn in a known Session, or creates a Session when absent.
    pub async fn run_turn_in_session(
        &self,
        definition_id: &str,
        session_id: Option<&str>,
        input: &str,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let session_id = if let Some(session_id) = session_id {
            session_id.to_owned()
        } else {
            let create_id = self.operations.command_id("create")?;
            self.host
                .create_session(&create_id, definition_id)
                .map_err(|_| DesktopHostError::HostFailure)?
                .session_id
        };
        let turn_id = self.operations.command_id("turn")?;
        let turn = self
            .host
            .start_turn(&turn_id, &session_id, input)
            .map_err(|_| DesktopHostError::HostFailure)?;
        self.finish_turn(session_id, turn).await
    }

    /// Continues one exact restart-safe text suspension and executes its fresh attempt.
    pub async fn continue_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        suspension_id: &str,
        session_version: u64,
        input: &str,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let command_id = self.operations.command_id("continue")?;
        let turn = self
            .host
            .continue_turn(
                &command_id,
                session_id,
                turn_id,
                suspension_id,
                session_version,
                HostContinuationInput::String(input),
            )
            .map_err(|_| DesktopHostError::HostFailure)?;
        self.finish_turn(session_id.to_owned(), turn).await
    }

    async fn finish_turn(
        &self,
        session_id: String,
        turn: TurnCommandResponse,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let attempt = self.operations.execution_attempt()?;
        self.queue
            .lock()
            .await
            .try_run_next(&self.worker, &attempt)
            .await
            .map_err(|_| DesktopHostError::ExecutionFailure)?;
        let page = self
            .host
            .read_event_page(&session_id, turn.committed_position)
            .map_err(|_| DesktopHostError::HostFailure)?;
        let terminal = page
            .events
            .iter()
            .find_map(|event| terminal(event.event.as_str()).map(|kind| (kind, event)))
            .ok_or(DesktopHostError::ProjectionFailure)?;
        Ok(DesktopTurnResult {
            session_id,
            turn_id: turn.turn_id,
            execution_id: turn.execution_id,
            cursor: page.scanned_through_position,
            terminal: terminal.0,
            text: terminal.1.text.clone(),
        })
    }

    /// Returns the most recent durable Sessions from the embedded Runtime.
    pub fn recent_sessions(
        &self,
        limit: usize,
    ) -> Result<Vec<DesktopSessionSummary>, DesktopHostError> {
        self.host
            .list_sessions(limit)
            .map_err(|_| DesktopHostError::ProjectionFailure)
    }

    /// Restores one durable conversation timeline from the embedded Runtime.
    pub fn session_timeline(
        &self,
        session_id: &str,
        after_position: u64,
        limit: usize,
    ) -> Result<DesktopTimelinePage, DesktopHostError> {
        self.host
            .read_timeline(session_id, after_position, limit)
            .map_err(|_| DesktopHostError::ProjectionFailure)
    }
}

fn terminal(event: &str) -> Option<DesktopTerminal> {
    match event {
        "turn.completed" => Some(DesktopTerminal::Completed),
        "turn.suspended" => Some(DesktopTerminal::Suspended),
        "turn.stopped" => Some(DesktopTerminal::Stopped),
        "turn.failed" => Some(DesktopTerminal::Failed),
        _ => None,
    }
}

/// Installable Tauri state; composition must come from the Garive backend system.
#[derive(Default)]
pub struct DesktopState {
    host: std::sync::Mutex<Option<Arc<DesktopHost>>>,
}

impl DesktopState {
    /// Returns only capabilities the installed backend can currently prove.
    pub fn capabilities(&self) -> DesktopCapabilityManifest {
        let (configured, agent_definition_id, activity) =
            self.host.lock().map_or((false, None, false), |slot| {
                slot.as_ref().map_or((false, None, false), |host| {
                    (
                        true,
                        Some(host.definition_id.clone()),
                        host.supports_activity(),
                    )
                })
            });
        DesktopCapabilityManifest {
            configured,
            agent_definition_id,
            multi_turn: configured,
            durable_navigation: configured,
            activity,
            setup: false,
            workspaces: false,
            artifacts: false,
        }
    }

    /// Loads and installs one backend-only system composition when present.
    pub fn install_from(
        &self,
        provider: &dyn DesktopConfigurationProvider,
    ) -> Result<bool, DesktopConfigurationError> {
        let Some(config) = provider.load()? else {
            return Ok(false);
        };
        let host =
            DesktopHost::new(config).map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
        self.install(host)
            .map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
        Ok(true)
    }

    /// Installs an explicitly constructed embedded Runtime once.
    pub fn install(&self, host: DesktopHost) -> Result<(), DesktopHostError> {
        let mut slot = self
            .host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?;
        if slot.is_some() {
            return Err(DesktopHostError::InvalidConfiguration);
        }
        *slot = Some(Arc::new(host));
        Ok(())
    }

    /// Creates one durable empty Session for context attachment.
    pub fn create_session(&self, definition_id: &str) -> Result<String, DesktopHostError> {
        let host = self
            .host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?
            .clone()
            .ok_or(DesktopHostError::NotConfigured)?;
        host.create_session(definition_id)
    }

    /// Attaches one already reverified opaque Workspace to a durable Session.
    pub fn attach_workspace(
        &self,
        session_id: &str,
        workspace: &DesktopWorkspaceGrant,
    ) -> Result<DesktopWorkspaceAttachment, DesktopHostError> {
        let host = self
            .host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?
            .clone()
            .ok_or(DesktopHostError::NotConfigured)?;
        host.attach_workspace(session_id, workspace)
    }

    /// Restores durable path-free Workspace attachments for one Session.
    pub fn session_workspaces(
        &self,
        session_id: &str,
    ) -> Result<Vec<DesktopWorkspaceAttachment>, DesktopHostError> {
        let host = self
            .host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?
            .clone()
            .ok_or(DesktopHostError::NotConfigured)?;
        host.session_workspaces(session_id)
    }

    /// Runs one typed command or reports missing system configuration.
    pub async fn run_turn(
        &self,
        definition_id: &str,
        input: &str,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let host = self
            .host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?
            .clone()
            .ok_or(DesktopHostError::NotConfigured)?;
        host.run_turn(definition_id, input).await
    }

    /// Runs the non-`Send` Engine future on an isolated current-thread executor.
    pub async fn run_turn_isolated(
        &self,
        definition_id: String,
        input: String,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        self.run_turn_in_session_isolated(definition_id, None, input)
            .await
    }

    /// Runs a multi-turn Desktop command on an isolated current-thread executor.
    pub async fn run_turn_in_session_isolated(
        &self,
        definition_id: String,
        session_id: Option<String>,
        input: String,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let host = self
            .host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?
            .clone()
            .ok_or(DesktopHostError::NotConfigured)?;
        tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| DesktopHostError::InvalidConfiguration)?;
            runtime.block_on(host.run_turn_in_session(
                &definition_id,
                session_id.as_deref(),
                &input,
            ))
        })
        .await
        .map_err(|_| DesktopHostError::ExecutionFailure)?
    }

    /// Continues a restart-safe text suspension on an isolated executor.
    pub async fn continue_turn_isolated(
        &self,
        session_id: String,
        turn_id: String,
        suspension_id: String,
        session_version: u64,
        input: String,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let host = self.installed_host()?;
        tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| DesktopHostError::InvalidConfiguration)?;
            runtime.block_on(host.continue_turn(
                &session_id,
                &turn_id,
                &suspension_id,
                session_version,
                &input,
            ))
        })
        .await
        .map_err(|_| DesktopHostError::ExecutionFailure)?
    }

    /// Returns recent durable Sessions or reports missing system configuration.
    pub fn recent_sessions(
        &self,
        limit: usize,
    ) -> Result<Vec<DesktopSessionSummary>, DesktopHostError> {
        self.installed_host()?.recent_sessions(limit)
    }

    /// Restores one durable Session timeline or reports missing configuration.
    pub fn session_timeline(
        &self,
        session_id: &str,
        after_position: u64,
        limit: usize,
    ) -> Result<DesktopTimelinePage, DesktopHostError> {
        self.installed_host()?
            .session_timeline(session_id, after_position, limit)
    }

    fn installed_host(&self) -> Result<Arc<DesktopHost>, DesktopHostError> {
        self.host
            .lock()
            .map_err(|_| DesktopHostError::InvalidConfiguration)?
            .clone()
            .ok_or(DesktopHostError::NotConfigured)
    }
}
