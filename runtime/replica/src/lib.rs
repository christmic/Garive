//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]

mod fake_host;
mod sqlite_ledger;

pub use fake_host::{FakeHost, HostEvent, HostEventKind};
pub use sqlite_ledger::{SqliteLedger, SqliteLedgerError};
