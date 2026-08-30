//! Embedded local Runtime composition behind typed Desktop IPC values.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::{path::PathBuf, sync::Arc};

use garive_llm::ModelPort;
use garive_runtime::{
    local_dispatch_queue, HostClock, InstalledAgent, LiveHost, LiveHostLimits, LocalDispatchQueue,
    LocalExecutionAttempt, LocalExecutionPolicy, LocalExecutionWorker,
};
use serde::Serialize;
use tokio::sync::Mutex;

mod setup;
mod system_configuration;
mod system_provider;

pub use setup::{
    DesktopSetupCatalogue, DesktopSetupError, DesktopSetupInput, DesktopSetupPlan,
    DesktopSetupProfile, DesktopSetupReceipt, DesktopSetupService, DesktopSetupSummary,
    SetupCredentialStore, SystemSetupCredentialStore,
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
            worker,
            queue: Mutex::new(queue),
            operations: config.operations,
        })
    }

    /// Creates a Session and executes one Turn through the embedded durable loop.
    pub async fn run_turn(
        &self,
        definition_id: &str,
        input: &str,
    ) -> Result<DesktopTurnResult, DesktopHostError> {
        let create_id = self.operations.command_id("create")?;
        let session = self
            .host
            .create_session(&create_id, definition_id)
            .map_err(|_| DesktopHostError::HostFailure)?;
        let turn_id = self.operations.command_id("turn")?;
        let turn = self
            .host
            .start_turn(&turn_id, &session.session_id, input)
            .map_err(|_| DesktopHostError::HostFailure)?;
        let attempt = self.operations.execution_attempt()?;
        self.queue
            .lock()
            .await
            .try_run_next(&self.worker, &attempt)
            .await
            .map_err(|_| DesktopHostError::ExecutionFailure)?;
        let page = self
            .host
            .read_event_page(&session.session_id, turn.committed_position)
            .map_err(|_| DesktopHostError::HostFailure)?;
        let terminal = page
            .events
            .iter()
            .find_map(|event| terminal(event.event.as_str()).map(|kind| (kind, event)))
            .ok_or(DesktopHostError::ProjectionFailure)?;
        Ok(DesktopTurnResult {
            session_id: session.session_id,
            turn_id: turn.turn_id,
            execution_id: turn.execution_id,
            cursor: page.scanned_through_position,
            terminal: terminal.0,
            text: terminal.1.text.clone(),
        })
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
            runtime.block_on(host.run_turn(&definition_id, &input))
        })
        .await
        .map_err(|_| DesktopHostError::ExecutionFailure)?
    }
}
