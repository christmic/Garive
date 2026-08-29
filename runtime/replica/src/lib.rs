//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod core_bridge;
mod fake_host;
mod runtime_turn;
mod sqlite_ledger;

pub use core_bridge::{
    canonical_model_request_digest, execute_durable_model_only, plan_core_terminal,
    plan_model_prepared, plan_model_started, plan_model_terminal, plan_model_uncertain,
    CoreTerminalContext, DurableExecutionConfig, DurableExecutionError, DurableExecutionResult,
    ModelLifecycleContext, PreparedModelRequest, RuntimeModelUncertainReason,
    TerminalPublicationError, TerminalPublisher,
};
pub use fake_host::{FakeHost, HostEvent, HostEventKind};
pub use runtime_turn::{
    plan_cancel_turn, plan_continue_turn, plan_recovery_restart, plan_start_turn,
    reconstruct_suspended_turn, select_runtime_recovery, CancelReason, CancelTurnCommand,
    ContinueTurnCommand, EffectRecoveryPosition, EffectiveRuntimeLimits, ExecutionRecoveryPosition,
    InteractionContinuation, ModelRecoveryPosition, PlannedTurn, RecoveryRestartCommand,
    RuntimeCommandError, RuntimeCommandId, RuntimeRecoveryAction, RuntimeRecoverySnapshot,
    StartTurnCommand, SuspendedTurnState,
};
pub use sqlite_ledger::{SqliteLedger, SqliteLedgerError};
