//! Bounded Agent kernel: decisions, turn semantics, and execution ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod context;
mod governed_execution;
mod memory_context;
mod model_only;
mod model_only_support;
mod model_only_types;
mod turn;

pub use context::{
    assemble_model_inputs, derive_context, merge_context_candidates, CandidateKind,
    ContextCandidate, ContextDerivationError, ContextItem, ContextPurpose, ContextRequest,
    ContextSurface, FactRef, Retention, Visibility,
};
pub use governed_execution::{
    execute_agent, execute_agent_with_preparation, AgentToolCapabilities, CommittedGovernedResult,
    GovernedEffectFuture, GovernedEffectPort, GovernedSuspensionBinding, ToolPreparationPort,
};
pub use memory_context::{
    derive_context_with_memory, MemoryContextError, MemoryContextItem, MemoryContextState,
    MemoryRecallContextBatch, MemoryRecallProduct,
};
pub use model_only::execute_model_only;
pub use model_only_types::{
    AgentCursor, AgentEntry, AgentEvent, AgentEventKind, AgentExecutionPorts, AgentFailureReason,
    AgentOutcome, AgentRequestError, AgentTurnRequest, ClockPort, ContextAdvance, ContextPort,
    ContextPortError, EventSink, ExecutionReport, MissingUsagePolicy, ModelOnlyLimits,
    ModelRecoveryPolicy, OutputLimitAction, PortFailure, ResumeInput, StopReason, SuspensionReason,
    TerminalRecoveryAction, UsageSummary,
};
pub use turn::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, BeginIteration, ControlError,
    ExecutionControl, ExecutionId, ExecutionLimits, ExecutionOutcomeKind, ExecutionStatus,
    InvalidIdentity, SessionId, TurnId,
};
