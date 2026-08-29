//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod core_bridge;
mod delegation_runtime;
mod live_host;
mod local_composition;
mod local_recovery;
mod local_worker;
mod model_http_transport;
mod observability_runtime;
mod runtime_turn;
mod scheduler_runtime;
mod sqlite_ledger;

pub use core_bridge::{
    authorize_memory_query, authorize_memory_write, canonical_model_request_digest,
    derive_knowledge_recovery, execute_durable_agent, execute_durable_agent_with_capabilities,
    execute_durable_agent_with_skill_activation, execute_durable_model_only,
    execute_durable_model_only_with_capabilities, execute_durable_model_only_with_skill_activation,
    plan_core_terminal, plan_knowledge_completed, plan_knowledge_dispatched, plan_knowledge_failed,
    plan_knowledge_requested, plan_memory_audit, plan_memory_distillation_checkpoint,
    plan_memory_erasure_receipt, plan_memory_forget, plan_memory_maintenance_decision,
    plan_memory_obligation, plan_memory_observation, plan_memory_promotion_receipt,
    plan_memory_promotion_request, plan_memory_recall, plan_memory_retrieval,
    plan_memory_tombstone, plan_memory_write, plan_model_prepared, plan_model_started,
    plan_model_terminal, plan_model_uncertain, plan_schedule_cancelled, plan_schedule_claimed,
    plan_schedule_created, plan_schedule_exhausted, plan_schedule_failed, plan_schedule_fired,
    plan_schedule_skipped, plan_skill_activation, reconstruct_memory_hypothesis_projection,
    reconstruct_memory_state, verify_memory_evidence, AuthorityDecision, AuthorityFuture,
    AuthorityPort, AuthorityRequest, CoreTerminalContext, DurableExecutionConfig,
    DurableExecutionError, DurableExecutionResult, ExecutorDispatch, ExecutorDispatchError,
    ExecutorFuture, ExecutorPort, GovernedEffectConfig, GovernedRuntimePortError,
    KnowledgeAccessGrant, KnowledgeConnector, KnowledgeConnectorFuture, KnowledgeConnectorOutcome,
    KnowledgeFailurePhase, KnowledgeFailureReason, KnowledgeLifecycleContext,
    KnowledgeRecoveryAction, KnowledgeRecoveryContext, MemoryAccessGrant,
    MemoryHypothesisProjection, MemoryMaintenanceContext, MemoryObligationContext,
    MemoryObservationContext, MemoryPrefix, MemoryRecallContext, MemoryRetrievalContext,
    MemoryTombstoneContext, MemoryTombstoneReason, MemoryWriteContext, MemoryWriteDecision,
    MemoryWriteRejection, ModelLifecycleContext, PlannedKnowledgeCompletion,
    PlannedMemoryObservation, PlannedMemoryPromotion, PlannedMemoryRecall, PlannedMemoryRetrieval,
    PlannedMemoryTombstone, PlannedMemoryWrite, PlannedSkillActivation, PreparedAgentCapabilities,
    PreparedExecution, PreparedKnowledgeCapability, PreparedKnowledgeRequest, PreparedModelRequest,
    RecordedMemoryRecall, RuntimeModelUncertainReason, ScheduleCancelReason,
    ScheduleDispatchDisposition, ScheduleLifecycleContext, SkillActivationContext,
    SqliteGovernedEffectPort, TerminalPublicationError, TerminalPublisher,
};
pub use delegation_runtime::{
    plan_delegation_authorization, plan_delegation_child_cancellation, plan_delegation_child_start,
    plan_delegation_child_terminal, plan_delegation_denial, plan_delegation_observation,
    plan_delegation_request, DelegationChildStartCommand, DelegationRuntimeError,
};
pub use live_host::{
    CommittedTurn, CreateSessionResponse, HostClock, HostEventPage, InstalledAgent, LiveHost,
    LiveHostError, LiveHostEvent, LiveHostLimits, LiveHostServer, LiveHostServerError,
    TurnCommandResponse, TurnDispatchError, TurnDispatcher,
};
pub use local_composition::{
    reconstruct_local_start, LocalExecutionAttempt, LocalExecutionPolicy, LocalReconstructionError,
    ReconstructedLocalExecution,
};
pub use local_recovery::{recover_local_dispatches, LocalRecoveryError};
pub use local_worker::{
    local_dispatch_queue, LocalDispatchQueue, LocalExecutionWorker, LocalTurnDispatcher,
    LocalWorkerDisposition, LocalWorkerError, LocalWorkerShutdownReport,
};
pub use model_http_transport::{
    RuntimeHttpLimits, RuntimeHttpTransportError, RuntimeModelHttpTransport,
};
pub use observability_runtime::{
    EnqueueDisposition, ObservabilityBuffer, ObservabilityLimits, ObservabilityRuntimeError,
    ObservabilitySink, RedactionPolicy, ShutdownReport, SinkDisposition,
};
pub use runtime_turn::{
    commit_planned_turn, derive_runtime_recovery, get_turn, plan_cancel_turn, plan_continue_turn,
    plan_reconcile_invocation, plan_recovery_action_facts, plan_recovery_restart, plan_start_turn,
    reconstruct_suspended_turn, select_runtime_recovery, CancelReason, CancelTurnCommand,
    ContinuationInput, ContinueTurnCommand, DelegationContinuation, EffectRecoveryPosition,
    EffectiveRuntimeLimits, ExecutionRecoveryPosition, GetTurnQuery, InteractionContinuation,
    InteractionExpiry, ModelRecoveryPosition, PlannedTurn, ReconcileInvocationCommand,
    ReconciliationDecision, ReconciliationTarget, RecoveryRestartCommand, RuntimeCommandError,
    RuntimeCommandId, RuntimeRecoveryAction, RuntimeRecoverySnapshot, RuntimeSuspensionKind,
    RuntimeSuspensionView, RuntimeTurnStatus, RuntimeTurnView, StartTurnCommand,
    SuspendedTurnState,
};
pub use scheduler_runtime::{
    cancel_schedule, create_schedule, reconstruct_schedule_state, run_schedule_once,
    update_schedule, PendingScheduleClaim, ScheduleAuthorityOperation, ScheduleAuthorityPort,
    ScheduleClock, ScheduleClockReading, ScheduleCommandDispatcher, ScheduleCommandReceipt,
    ScheduleRuntimeState, ScheduleTickConfig, ScheduleTickOutcome,
};
pub use sqlite_ledger::{
    ExecutionLease, ExecutionLeaseError, ExecutionLeaseRequest, ScheduleLease, ScheduleLeaseError,
    ScheduleLeaseRequest, SessionWatermark, SqliteLedger, SqliteLedgerError,
};
