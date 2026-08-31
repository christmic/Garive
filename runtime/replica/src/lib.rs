//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(unix)]
mod builtin_workspace_executor;
#[cfg(unix)]
mod confined_read_executor;
mod core_bridge;
mod delegation_runtime;
mod effect_batch_facts;
mod effect_batch_recovery;
mod effect_batch_runtime;
mod effect_batch_sqlite;
mod goal_recovery;
mod goal_runtime;
mod live_host;
mod local_composition;
mod local_recovery;
mod local_worker;
mod memory_control;
mod memory_export;
mod memory_export_io;
mod model_http_transport;
mod native_capability;
mod native_cdp;
mod native_cdp_port;
mod native_executor;
mod observability_runtime;
mod plan_carry_forward;
mod plan_recovery;
mod plan_runtime;
mod runtime_turn;
mod sandbox_facts;
mod sandbox_recovery;
mod sandbox_runtime;
mod scheduler_runtime;
mod sqlite_ledger;

#[cfg(unix)]
pub use builtin_workspace_executor::{BuiltinWorkspaceExecutor, T1_WORKSPACE_EXECUTOR_ID};
#[cfg(unix)]
pub use confined_read_executor::ConfinedFileReadExecutor;
pub use core_bridge::{
    authorize_memory_query, authorize_memory_write, canonical_model_request_digest,
    decode_committed_memory_recall, derive_knowledge_recovery, execute_durable_agent,
    execute_durable_agent_with_capabilities, execute_durable_agent_with_f0,
    execute_durable_agent_with_skill_activation, execute_durable_model_only,
    execute_durable_model_only_with_capabilities, execute_durable_model_only_with_skill_activation,
    plan_classified_memory_write, plan_core_terminal, plan_knowledge_completed,
    plan_knowledge_dispatched, plan_knowledge_failed, plan_knowledge_requested,
    plan_memory_archive, plan_memory_audit, plan_memory_distillation_checkpoint,
    plan_memory_erasure_receipt, plan_memory_forget, plan_memory_maintenance_decision,
    plan_memory_obligation, plan_memory_observation, plan_memory_promotion_receipt,
    plan_memory_promotion_request, plan_memory_recall, plan_memory_repository_import,
    plan_memory_retrieval, plan_memory_tombstone, plan_memory_write, plan_model_prepared,
    plan_model_started, plan_model_terminal, plan_model_uncertain, plan_schedule_cancelled,
    plan_schedule_claimed, plan_schedule_created, plan_schedule_exhausted, plan_schedule_failed,
    plan_schedule_fired, plan_schedule_skipped, plan_skill_activation,
    reconstruct_memory_hypothesis_projection, reconstruct_memory_maintenance_projection,
    reconstruct_memory_repository, reconstruct_memory_repository_projection,
    reconstruct_memory_state, verify_memory_evidence, AuthorityDecision, AuthorityFuture,
    AuthorityPort, AuthorityRequest, CoreTerminalContext, DurableExecutionConfig,
    DurableExecutionError, DurableExecutionResult, ExecutorDispatch, ExecutorDispatchError,
    ExecutorFuture, ExecutorPort, F0ExecutionGovernance, F0GovernanceContext, GovernedEffectConfig,
    GovernedRuntimePortError, KnowledgeAccessGrant, KnowledgeConnector, KnowledgeConnectorFuture,
    KnowledgeConnectorOutcome, KnowledgeFailurePhase, KnowledgeFailureReason,
    KnowledgeLifecycleContext, KnowledgeRecoveryAction, KnowledgeRecoveryContext,
    MemoryAccessGrant, MemoryHypothesisProjection, MemoryMaintenanceContext,
    MemoryMaintenanceProjection, MemoryObligationContext, MemoryObservationContext, MemoryPrefix,
    MemoryRecallContext, MemoryRetrievalContext, MemoryTombstoneContext, MemoryTombstoneReason,
    MemoryWriteContext, MemoryWriteDecision, MemoryWriteRejection, ModelLifecycleContext,
    PlannedKnowledgeCompletion, PlannedMemoryArchive, PlannedMemoryForget,
    PlannedMemoryObservation, PlannedMemoryPromotion, PlannedMemoryRecall,
    PlannedMemoryRepositoryImport, PlannedMemoryRetrieval, PlannedMemoryTombstone,
    PlannedMemoryWrite, PlannedSkillActivation, PreparedAgentCapabilities, PreparedExecution,
    PreparedKnowledgeCapability, PreparedKnowledgeRequest, PreparedModelRequest,
    RecordedMemoryDecision, RecordedMemoryErasure, RecordedMemoryRecall, RecoveredMemoryRepository,
    RuntimeModelUncertainReason, SafetyEvaluation, SafetyFuture, SafetyInteraction, SafetyPort,
    SandboxAdmission, SandboxAdmissionPort, SandboxAdmissionRequest, ScheduleCancelReason,
    ScheduleDispatchDisposition, ScheduleLifecycleContext, SkillActivationContext,
    SqliteGovernedEffectPort, TerminalPublicationError, TerminalPublisher,
};
pub use delegation_runtime::{
    plan_delegation_authorization, plan_delegation_child_cancellation, plan_delegation_child_start,
    plan_delegation_child_terminal, plan_delegation_denial, plan_delegation_observation,
    plan_delegation_request, DelegationChildStartCommand, DelegationRuntimeError,
};
pub use effect_batch_facts::{
    plan_effect_batch_admission, EffectBatchAdmissionContext, PlannedEffectBatchAdmission,
};
pub use effect_batch_recovery::{
    reconstruct_effect_batch_recovery, EffectBatchMemberRecovery, RecoveredEffectBatch,
};
pub use effect_batch_runtime::{
    AuthorizedBatchInvocation, BatchRuntimeError, BatchTerminal, CancellationEvidence,
    ConcurrentExecutorDispatch, ConcurrentExecutorPort, EffectBatchDispatcher,
    EffectBatchPublisher, EffectBatchReport, EffectBatchRuntimeLimits, EffectCancellation,
};
pub use effect_batch_sqlite::SqliteEffectBatchPublisher;
pub use goal_recovery::reconstruct_goal;
pub use goal_runtime::{
    commit_goal_command, plan_create_goal, plan_goal_transition, GoalCommandContext,
    GoalRuntimeError, GoalRuntimeState, GoalRuntimeTransition, PlannedGoalCommand,
};
pub use live_host::{
    ActivityProjectionLimits, AgentDefinitionPageV1, AgentDefinitionSummary,
    AgentDefinitionSummaryV1, CommittedTurn, CreateSessionResponse, HostActivity, HostArtifact,
    HostArtifactPage, HostClock, HostContinuationInput, HostEventPage, HostReadLimits,
    HostWorkspaceAttachment, HostWorkspaceContextEntry, HostWorkspaceDetachment,
    InstalledActivityCatalogue, InstalledActivityDescriptor, InstalledAgent, LiveHost,
    LiveHostError, LiveHostEvent, LiveHostLimits, LiveHostServer, LiveHostServerError,
    SessionPageV1, SessionSummary, SessionSummaryV1, SessionViewV1, SuspensionViewV1,
    TurnCommandResponse, TurnDispatchError, TurnDispatcher, TurnSuspensionView, TurnTimelineItem,
    TurnTimelineItemV1, TurnTimelinePage, TurnTimelinePageV1,
};
pub use local_composition::{
    reconstruct_local_start, LocalExecutionAttempt, LocalExecutionPolicy, LocalReconstructionError,
    ReconstructedLocalExecution,
};
pub use local_recovery::{
    recover_local_dispatches, recover_local_dispatches_with_f0, LocalRecoveryError,
};
pub use local_worker::{
    local_dispatch_queue, LocalDispatchQueue, LocalExecutionWorker, LocalF0Governance,
    LocalGovernedExecution, LocalGovernedExecutionFactory, LocalTurnDispatcher,
    LocalWorkerDisposition, LocalWorkerError, LocalWorkerShutdownReport,
};
pub use memory_control::{
    MemoryControlAction, MemoryControlGrant, MemoryControlProjection, MemoryControlRuntimeError,
    MemoryImportCommand, MemoryImportReceipt, MemoryRepositoryCommitResult,
    MemoryRepositoryErasurePolicy, MemoryRepositoryError, MemoryRepositoryImportContext,
    MemoryRepositoryImportPolicy, MemoryRepositoryStatus,
};
pub use memory_export::{MemoryExportCommand, MemoryExportReceipt, MemoryExportTarget};
pub use memory_export_io::export_memory_snapshot;
pub use model_http_transport::{
    RuntimeHttpLimits, RuntimeHttpTransportError, RuntimeModelHttpTransport,
    RUNTIME_MODEL_HTTP_TRANSPORT_REVISION,
};
pub use native_capability::{
    ApplicationId, BrowserPageId, BrowserSessionId, DesktopSessionId, NativeActionCommandV1,
    NativeActionFuture, NativeActionId, NativeActionReceiptV1, NativeAdapterBindingV1,
    NativeAdapterPort, NativeNodeRef, NativeObservationBounds, NativeObservationFuture,
    NativeObservationV1, NativeProtocolError, NativeSemanticNode, NativeSensitivity,
    NativeSnapshotId, NativeTarget, WindowId,
};
pub use native_cdp::{
    map_cdp_ax_tree, map_cdp_ax_tree_with_binding, map_cdp_ax_tree_with_frame_scope,
    CdpElementTarget, CdpFrameScope, CdpObservationContext, CdpSnapshotBindingV1,
    MappedCdpObservation,
};
pub use native_cdp_port::CdpNativeAdapterPort;
pub use native_executor::{NativeCapabilityExecutor, T2_NATIVE_EXECUTOR_ID};
pub use observability_runtime::{
    EnqueueDisposition, ObservabilityBuffer, ObservabilityLimits, ObservabilityRuntimeError,
    ObservabilitySink, RedactionPolicy, ShutdownReport, SinkDisposition,
};
pub use plan_carry_forward::{
    commit_plan_replacement, plan_plan_replacement, verify_plan_carry_forward,
    PlannedPlanReplacement, VerifiedPlanCarryForward,
};
pub use plan_recovery::reconstruct_plan;
pub use plan_runtime::{
    commit_plan_command, plan_plan_transition, plan_propose_plan, plan_start_step_execution,
    ActivePlanClaim, PlanCommandContext, PlanRetryPosture, PlanRuntimeError, PlanRuntimeState,
    PlanRuntimeTransition, PlanStepExecutionStart, PlannedPlanCommand,
};
pub use runtime_turn::{
    commit_planned_turn, derive_runtime_recovery, get_turn, plan_cancel_turn, plan_continue_turn,
    plan_reconcile_invocation, plan_recovery_action_facts, plan_recovery_restart, plan_start_turn,
    reconstruct_suspended_turn, select_runtime_recovery, CancelReason, CancelTurnCommand,
    ContinuationInput, ContinueTurnCommand, DelegationContinuation, EffectRecoveryPosition,
    EffectiveRuntimeLimits, ExecutionRecoveryPosition, GetTurnQuery, InteractionContinuation,
    InteractionExpiry, InteractionInputRepresentation, ModelRecoveryPosition, PlannedTurn,
    ReconcileInvocationCommand, ReconciliationDecision, ReconciliationTarget,
    RecoveryRestartCommand, RuntimeCommandError, RuntimeCommandId, RuntimeRecoveryAction,
    RuntimeRecoverySnapshot, RuntimeSuspensionKind, RuntimeSuspensionView, RuntimeTurnStatus,
    RuntimeTurnView, StartTurnCommand, SuspendedTurnState,
};
pub use sandbox_facts::{
    plan_f0_effect_admission, plan_f0_prepared, plan_f0_safety_decision, plan_f0_sandbox_admission,
    F0EffectAdmissionContext, F0SafetyDecisionContext, PlannedF0EffectAdmission,
};
pub use sandbox_recovery::{
    recover_f0_prepared, recover_f0_prepared_with_port, F0RecoveryContentPort, F0RecoveryError,
    RecoveredF0Prepared,
};
pub use sandbox_runtime::{
    preflight_sandbox, SafetyDecisionV1, SafetyDisposition, SafetyRequestV1, SandboxBindingV1,
    SandboxPreflightError,
};
pub use scheduler_runtime::{
    cancel_schedule, create_schedule, reconstruct_schedule_state, run_schedule_once,
    update_schedule, PendingScheduleClaim, ScheduleAuthorityOperation, ScheduleAuthorityPort,
    ScheduleClock, ScheduleClockReading, ScheduleCommandDispatcher, ScheduleCommandReceipt,
    ScheduleRuntimeState, ScheduleTickConfig, ScheduleTickOutcome,
};
pub use sqlite_ledger::{
    ExecutionLease, ExecutionLeaseError, ExecutionLeaseRequest, MemoryRepositoryCommitError,
    MemoryRepositoryImportCommitError, ScheduleLease, ScheduleLeaseError, ScheduleLeaseRequest,
    SessionWatermark, SqliteLedger, SqliteLedgerError,
};
