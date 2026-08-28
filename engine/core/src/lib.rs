//! Bounded Agent kernel: decisions, turn semantics, and execution ports.

#![forbid(unsafe_code)]

mod turn;

pub use turn::{
    BeginIteration, ControlError, ExecutionControl, ExecutionId, ExecutionLimits,
    ExecutionOutcomeKind, ExecutionStatus, InvalidIdentity, TurnId,
};
