//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod fake_host;
mod sqlite_ledger;

pub use fake_host::{FakeHost, HostEvent, HostEventKind};
pub use sqlite_ledger::{SqliteLedger, SqliteLedgerError};
