//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod fake_host;
mod runtime_turn;
mod sqlite_ledger;

pub use fake_host::{FakeHost, HostEvent, HostEventKind};
pub use runtime_turn::{
    plan_cancel_turn, plan_start_turn, CancelReason, CancelTurnCommand, EffectiveRuntimeLimits,
    PlannedTurn, RuntimeCommandError, RuntimeCommandId, StartTurnCommand,
};
pub use sqlite_ledger::{SqliteLedger, SqliteLedgerError};
