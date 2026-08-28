//! Bounded Agent kernel: decisions, turn semantics, and execution ports.

#![forbid(unsafe_code)]

mod turn;

pub use turn::{
    InvalidTurnId, IterationDecision, SuspensionReason, TerminalReason, TransitionError, TurnId,
    TurnLimits, TurnState, TurnStatus,
};
