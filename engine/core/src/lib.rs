//! Bounded Agent kernel: decisions, turn semantics, and execution ports.

#![forbid(unsafe_code)]

mod context;
mod turn;

pub use context::{
    derive_context, CandidateKind, ContextCandidate, ContextDerivationError, ContextItem,
    ContextPurpose, ContextRequest, ContextSurface, FactRef, Retention, Visibility,
};

pub use turn::{
    BeginIteration, ControlError, ExecutionControl, ExecutionId, ExecutionLimits,
    ExecutionOutcomeKind, ExecutionStatus, InvalidIdentity, TurnId,
};
