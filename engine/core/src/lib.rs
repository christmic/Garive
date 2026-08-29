//! Bounded Agent kernel: decisions, turn semantics, and execution ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod context;
mod governed_execution;
mod model_only;
mod model_only_support;
mod model_only_types;
mod turn;

pub use context::{
    derive_context, CandidateKind, ContextCandidate, ContextDerivationError, ContextItem,
    ContextPurpose, ContextRequest, ContextSurface, FactRef, Retention, Visibility,
};
pub use governed_execution::{
    execute_agent, AgentToolCapabilities, CommittedGovernedResult, GovernedEffectFuture,
    GovernedEffectPort, GovernedSuspensionBinding,
};
pub use model_only::execute_model_only;
pub use model_only_types::{
    AgentCursor, AgentEntry, AgentEvent, AgentEventKind, AgentExecutionPorts, AgentFailureReason,
    AgentOutcome, AgentRequestError, AgentTurnRequest, ClockPort, ContextPort, ContextPortError,
    EventSink, ExecutionReport, MissingUsagePolicy, ModelOnlyLimits, ModelRecoveryPolicy,
    OutputLimitAction, PortFailure, ResumeInput, StopReason, SuspensionReason,
    TerminalRecoveryAction, UsageSummary,
};
pub use turn::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, BeginIteration, ControlError,
    ExecutionControl, ExecutionId, ExecutionLimits, ExecutionOutcomeKind, ExecutionStatus,
    InvalidIdentity, SessionId, TurnId,
};
