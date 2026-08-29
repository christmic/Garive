//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod core_bridge;
mod fake_host;
mod live_host;
mod model_http_transport;
mod runtime_turn;
mod sqlite_ledger;

pub use core_bridge::{
    canonical_model_request_digest, execute_durable_agent,
    execute_durable_agent_with_skill_activation, execute_durable_model_only,
    execute_durable_model_only_with_skill_activation, plan_core_terminal, plan_model_prepared,
    plan_model_started, plan_model_terminal, plan_model_uncertain, plan_skill_activation,
    AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest, CoreTerminalContext,
    DurableExecutionConfig, DurableExecutionError, DurableExecutionResult, ExecutorDispatch,
    ExecutorDispatchError, ExecutorFuture, ExecutorPort, GovernedEffectConfig,
    GovernedRuntimePortError, ModelLifecycleContext, PlannedSkillActivation, PreparedExecution,
    PreparedModelRequest, RuntimeModelUncertainReason, SkillActivationContext,
    SqliteGovernedEffectPort, TerminalPublicationError, TerminalPublisher,
};
pub use fake_host::{FakeHost, HostEvent, HostEventKind};
pub use live_host::{
    CommittedTurn, CreateSessionResponse, HostClock, HostEventPage, InstalledAgent, LiveHost,
    LiveHostError, LiveHostEvent, LiveHostLimits, LiveHostServer, LiveHostServerError,
    TurnCommandResponse, TurnDispatchError, TurnDispatcher,
};
pub use model_http_transport::{
    RuntimeHttpLimits, RuntimeHttpTransportError, RuntimeModelHttpTransport,
};
pub use runtime_turn::{
    commit_planned_turn, derive_runtime_recovery, get_turn, plan_cancel_turn, plan_continue_turn,
    plan_reconcile_invocation, plan_recovery_action_facts, plan_recovery_restart, plan_start_turn,
    reconstruct_suspended_turn, select_runtime_recovery, CancelReason, CancelTurnCommand,
    ContinuationInput, ContinueTurnCommand, EffectRecoveryPosition, EffectiveRuntimeLimits,
    ExecutionRecoveryPosition, GetTurnQuery, InteractionContinuation, InteractionExpiry,
    ModelRecoveryPosition, PlannedTurn, ReconcileInvocationCommand, ReconciliationDecision,
    ReconciliationTarget, RecoveryRestartCommand, RuntimeCommandError, RuntimeCommandId,
    RuntimeRecoveryAction, RuntimeRecoverySnapshot, RuntimeSuspensionKind, RuntimeSuspensionView,
    RuntimeTurnStatus, RuntimeTurnView, StartTurnCommand, SuspendedTurnState,
};
pub use sqlite_ledger::{
    ExecutionLease, ExecutionLeaseError, ExecutionLeaseRequest, SessionWatermark, SqliteLedger,
    SqliteLedgerError,
};
