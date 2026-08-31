//! Bounded post-commit queue and one model-only local execution worker.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
        Arc,
    },
};

use garive_core::{AgentEvent, AgentToolCapabilities, ClockPort, EventSink, PortFailure};
use garive_core::{AgentOutcome, AgentTurnRequest, ToolPreparationPort};
use garive_llm::{ModelCancellation, ModelPort};

use crate::{
    execute_durable_agent_with_capabilities, execute_durable_model_only_with_capabilities,
    reconstruct_local_start, AuthorityPort, CommittedTurn, ExecutorPort, F0ExecutionGovernance,
    F0GovernanceContext, F0RecoveryContentPort, LiveOutputEndReason, LiveOutputHub, LiveOutputSink,
    LocalExecutionAttempt, LocalExecutionPolicy, LocalReconstructionError,
    PreparedAgentCapabilities, SafetyPort, SandboxAdmissionPort, SqliteLedger,
    TerminalPublicationError, TerminalPublisher, TurnDispatchError, TurnDispatcher,
};

/// Bounded non-blocking dispatcher installed behind [`crate::LiveHost`].
pub struct LocalTurnDispatcher {
    sender: SyncSender<CommittedTurn>,
    accepting: Arc<AtomicBool>,
}
impl LocalTurnDispatcher {
    /// Stops new queue admission without changing already committed Turns.
    pub fn stop_admission(&self) {
        self.accepting.store(false, Ordering::Release);
    }
}
impl TurnDispatcher for LocalTurnDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(TurnDispatchError);
        }
        self.sender
            .try_send(turn.clone())
            .map_err(|_| TurnDispatchError)
    }
}

/// Sole consumer of committed local Turn dispatches.
pub struct LocalDispatchQueue {
    receiver: Receiver<CommittedTurn>,
    accepting: Arc<AtomicBool>,
}
impl LocalDispatchQueue {
    /// Executes the next queued Turn without blocking when the queue is empty.
    pub async fn try_run_next(
        &mut self,
        worker: &LocalExecutionWorker,
        attempt: &LocalExecutionAttempt,
    ) -> Result<LocalWorkerDisposition, LocalWorkerError> {
        match self.receiver.try_recv() {
            Ok(committed) => worker.execute(&committed, attempt).await,
            Err(TryRecvError::Empty) => Err(LocalWorkerError::QueueEmpty),
            Err(TryRecvError::Disconnected) => Err(LocalWorkerError::WorkerStopped),
        }
    }

    /// Stops admission and consumes at most one queued Turn per supplied attempt.
    pub async fn shutdown_drain(
        &mut self,
        worker: &LocalExecutionWorker,
        attempts: &[LocalExecutionAttempt],
    ) -> LocalWorkerShutdownReport {
        self.accepting.store(false, Ordering::Release);
        let mut completed = 0;
        let mut failed = 0;
        for attempt in attempts {
            let committed = match self.receiver.try_recv() {
                Ok(value) => value,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            match worker.execute(&committed, attempt).await {
                Ok(_) => completed += 1,
                Err(_) => failed += 1,
            }
        }
        let mut abandoned = 0;
        while self.receiver.try_recv().is_ok() {
            abandoned += 1;
        }
        LocalWorkerShutdownReport {
            attempted: completed + failed,
            completed,
            failed,
            abandoned,
        }
    }
}

/// Creates one explicit non-zero bounded post-commit dispatch queue.
pub fn local_dispatch_queue(
    capacity: usize,
) -> Result<(Arc<LocalTurnDispatcher>, LocalDispatchQueue), LocalWorkerError> {
    if capacity == 0 {
        return Err(LocalWorkerError::InvalidComposition);
    }
    let (sender, receiver) = sync_channel(capacity);
    let accepting = Arc::new(AtomicBool::new(true));
    Ok((
        Arc::new(LocalTurnDispatcher {
            sender,
            accepting: accepting.clone(),
        }),
        LocalDispatchQueue {
            receiver,
            accepting,
        },
    ))
}

/// Model-only worker configured entirely from constructed Garive values.
pub struct LocalExecutionWorker {
    database_path: PathBuf,
    policy: LocalExecutionPolicy,
    model: Arc<dyn ModelPort>,
    governed: Option<Arc<dyn LocalGovernedExecutionFactory>>,
    capability_preparation: Option<Arc<dyn LocalCapabilityPreparationFactory>>,
    live_output: Option<LiveOutputHub>,
}

/// Fixed durable input exposed to one Runtime capability-preparation factory.
pub struct LocalCapabilityPreparationInput<'a> {
    /// Exact committed dispatch coordinates.
    pub committed: &'a CommittedTurn,
    /// Reconstructed Core request containing the trusted current input.
    pub request: &'a AgentTurnRequest,
    /// Canonical Runtime observation time for capability facts.
    pub recorded_at: &'a str,
}

/// Prepares snapshot-admitted capability inputs without committing facts.
pub trait LocalCapabilityPreparationFactory: Send + Sync {
    /// Resolves exact Runtime bindings against one read-only durable prefix.
    fn prepare(
        &self,
        ledger: &SqliteLedger,
        input: LocalCapabilityPreparationInput<'_>,
    ) -> Result<PreparedAgentCapabilities, LocalWorkerError>;
}

/// Frozen capabilities and governed ports created for one local Execution.
pub struct LocalGovernedExecution {
    /// Exact Tool definitions admitted to this Execution.
    pub capabilities: AgentToolCapabilities,
    /// Runtime-neutral authority implementation for prepared calls.
    pub authority: Box<dyn AuthorityPort>,
    /// Two-phase executor implementation for authorized calls.
    pub executor: Box<dyn ExecutorPort>,
    /// Mandatory v3 governance for every tool-capable local Execution.
    pub f0: LocalF0Governance,
}

/// Owned Safety/Sandbox composition frozen for one local Execution.
pub struct LocalF0Governance {
    /// Pure versioned Prepared-v3 resolver composition.
    pub preparation: Box<dyn ToolPreparationPort>,
    /// Bounded resolver for opaque Prepared argument content during restart.
    pub recovery_content: Box<dyn F0RecoveryContentPort>,
    /// Runtime Safety policy broker.
    pub safety: Box<dyn SafetyPort>,
    /// Runtime Sandbox selection and preflight broker.
    pub sandbox: Box<dyn SandboxAdmissionPort>,
    /// Authenticated authority and effective-policy bindings.
    pub context: F0GovernanceContext,
}

/// Constructs isolated governed ports for one committed local Execution.
pub trait LocalGovernedExecutionFactory: Send + Sync {
    /// Freezes capabilities and ports before model dispatch begins.
    fn create(&self, committed: &CommittedTurn)
        -> Result<LocalGovernedExecution, LocalWorkerError>;
}

impl LocalExecutionWorker {
    /// Constructs a worker without reading environment or configuration files.
    pub fn new(
        database_path: impl AsRef<Path>,
        policy: LocalExecutionPolicy,
        model: Arc<dyn ModelPort>,
    ) -> Result<Self, LocalWorkerError> {
        if database_path.as_ref().as_os_str().is_empty() {
            return Err(LocalWorkerError::InvalidComposition);
        }
        Ok(Self {
            database_path: database_path.as_ref().to_owned(),
            policy,
            model,
            governed: None,
            capability_preparation: None,
            live_output: None,
        })
    }

    /// Installs the explicit lossy H4 publication boundary.
    pub fn with_live_output(mut self, live_output: LiveOutputHub) -> Self {
        self.live_output = Some(live_output);
        self
    }

    /// Installs exact Runtime preparation for snapshot-admitted capabilities.
    pub fn with_capability_preparation(
        mut self,
        factory: Arc<dyn LocalCapabilityPreparationFactory>,
    ) -> Self {
        self.capability_preparation = Some(factory);
        self
    }

    /// Constructs a tool-capable worker with explicit governed port creation.
    pub fn new_governed(
        database_path: impl AsRef<Path>,
        policy: LocalExecutionPolicy,
        model: Arc<dyn ModelPort>,
        governed: Arc<dyn LocalGovernedExecutionFactory>,
    ) -> Result<Self, LocalWorkerError> {
        let mut worker = Self::new(database_path, policy, model)?;
        worker.governed = Some(governed);
        Ok(worker)
    }

    /// Reconstructs and executes one already committed start transaction.
    pub async fn execute(
        &self,
        committed: &CommittedTurn,
        attempt: &LocalExecutionAttempt,
    ) -> Result<LocalWorkerDisposition, LocalWorkerError> {
        let mut ledger = SqliteLedger::open(&self.database_path)
            .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
        let mut reconstructed =
            match reconstruct_local_start(&ledger, committed, &self.policy, attempt) {
                Ok(value) => value,
                Err(LocalReconstructionError::AlreadyTerminal) => {
                    return Ok(LocalWorkerDisposition::AlreadyTerminal)
                }
                Err(error) => return Err(LocalWorkerError::Reconstruction(error)),
            };
        let cancellation = NeverCancelled;
        let clock = FixedClock(attempt.now_ms);
        let mut events = match &self.live_output {
            Some(hub) => WorkerEvents::Live(hub.event_sink()),
            None => WorkerEvents::Discard(DiscardEvents),
        };
        let mut publisher = DurableOnlyPublisher;
        let prepared_capabilities = match &self.capability_preparation {
            Some(factory) => factory.prepare(
                &ledger,
                LocalCapabilityPreparationInput {
                    committed,
                    request: &reconstructed.request,
                    recorded_at: &attempt.recorded_at,
                },
            )?,
            None => PreparedAgentCapabilities::default(),
        };
        let execution = if let Some(factory) = &self.governed {
            let mut governed = factory.create(committed)?;
            if governed.capabilities.definitions.is_empty() {
                return Err(LocalWorkerError::InvalidComposition);
            }
            let mut f0 = governed.f0;
            execute_durable_agent_with_capabilities(
                &mut ledger,
                &reconstructed.durable,
                &reconstructed.request,
                prepared_capabilities,
                &governed.capabilities,
                &mut reconstructed.context,
                self.model.as_ref(),
                governed.authority.as_mut(),
                governed.executor.as_mut(),
                F0ExecutionGovernance {
                    preparation: f0.preparation.as_ref(),
                    safety: f0.safety.as_mut(),
                    sandbox: f0.sandbox.as_mut(),
                    context: f0.context,
                },
                &mut events,
                &cancellation,
                &clock,
                &mut publisher,
            )
            .await
        } else {
            execute_durable_model_only_with_capabilities(
                &mut ledger,
                &reconstructed.durable,
                &reconstructed.request,
                prepared_capabilities,
                &mut reconstructed.context,
                self.model.as_ref(),
                &mut events,
                &cancellation,
                &clock,
                &mut publisher,
            )
            .await
        };
        let result = match execution {
            Ok(result) => result,
            Err(_) => {
                self.end_live(committed, LiveOutputEndReason::PublisherClosed);
                return Err(LocalWorkerError::ExecutionFailed);
            }
        };
        let live_end = match &result.report.outcome {
            AgentOutcome::Completed { .. } => LiveOutputEndReason::TerminalCommitted,
            AgentOutcome::Suspended { .. } => LiveOutputEndReason::Suspended,
            AgentOutcome::Stopped { .. } => LiveOutputEndReason::Stopped,
            AgentOutcome::Failed { .. } => LiveOutputEndReason::Failed,
        };
        self.end_live(committed, live_end);
        Ok(LocalWorkerDisposition::TerminalCommitted {
            positions: result.terminal_commit.positions,
        })
    }

    fn end_live(&self, committed: &CommittedTurn, reason: LiveOutputEndReason) {
        let Some(hub) = &self.live_output else {
            return;
        };
        let _ = hub.end_execution(
            committed.session_id.as_str(),
            committed.turn_id.as_str(),
            committed.execution_id.as_str(),
            reason,
        );
    }
}

/// Durable result of one consumed queue item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalWorkerDisposition {
    /// One terminal transaction committed at these Session positions.
    TerminalCommitted {
        /// Exact terminal transaction positions.
        positions: Vec<u64>,
    },
    /// Duplicate delivery found an already terminal Execution and did no work.
    AlreadyTerminal,
}

/// Bounded local shutdown result; abandoned Turns remain durable for restart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalWorkerShutdownReport {
    /// Queue items for which execution was attempted.
    pub attempted: usize,
    /// Items that completed or were already terminal.
    pub completed: usize,
    /// Items whose bounded worker attempt failed.
    pub failed: usize,
    /// Items removed from memory after the explicit drain bound.
    pub abandoned: usize,
}

/// Stable secret-free local queue or worker failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalWorkerError {
    /// Constructor or explicit queue values are invalid.
    InvalidComposition,
    /// No committed item is currently queued.
    QueueEmpty,
    /// The sole queue receiver or all dispatch senders are gone.
    WorkerStopped,
    /// SQLite could not open or verify required state.
    DurabilityUnavailable,
    /// Fixed-prefix reconstruction rejected the queued coordinates.
    Reconstruction(LocalReconstructionError),
    /// The installed snapshot requires a capability without a system binding.
    CapabilityBindingMissing,
    /// The installed descriptor and explicit system binding differ.
    CapabilityBindingMismatch,
    /// Durable Memory facts and the current projection disagree.
    MemoryRepositoryCorrupt,
    /// A configured Memory repository scan bound was exceeded.
    MemoryRepositoryBoundExceeded,
    /// An authorized Memory query or retrieval could not be constructed.
    MemoryPreparationFailed,
    /// Durable Core execution did not reach a committed terminal.
    ExecutionFailed,
}
impl LocalWorkerError {
    /// Returns the stable operational code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidComposition => "invalid_composition",
            Self::QueueEmpty => "dispatch_queue_empty",
            Self::WorkerStopped => "worker_stopped",
            Self::DurabilityUnavailable => "durability_unavailable",
            Self::Reconstruction(_) => "reconstruction_failed",
            Self::CapabilityBindingMissing => "capability_binding_missing",
            Self::CapabilityBindingMismatch => "capability_binding_mismatch",
            Self::MemoryRepositoryCorrupt => "memory_repository_corrupt",
            Self::MemoryRepositoryBoundExceeded => "memory_repository_bound_exceeded",
            Self::MemoryPreparationFailed => "memory_preparation_failed",
            Self::ExecutionFailed => "execution_failed",
        }
    }
}

struct NeverCancelled;
impl ModelCancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

struct FixedClock(u64);
impl ClockPort for FixedClock {
    fn now_tick(&self) -> Result<u64, PortFailure> {
        Ok(self.0)
    }
}

struct DiscardEvents;
impl EventSink for DiscardEvents {
    fn emit(&mut self, _: AgentEvent) -> Result<(), PortFailure> {
        Ok(())
    }
}

enum WorkerEvents {
    Live(LiveOutputSink),
    Discard(DiscardEvents),
}
impl EventSink for WorkerEvents {
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure> {
        match self {
            Self::Live(sink) => sink.emit(event),
            Self::Discard(sink) => sink.emit(event),
        }
    }
}

struct DurableOnlyPublisher;
impl TerminalPublisher for DurableOnlyPublisher {
    fn publish_terminal(
        &mut self,
        _: &garive_core::ExecutionReport,
        _: &[u64],
    ) -> Result<(), TerminalPublicationError> {
        Ok(())
    }
}
