//! Garive's Runtime composition root and native persistence adapters.

#![forbid(unsafe_code)]

mod sqlite_ledger;

pub use sqlite_ledger::{SqliteLedger, SqliteLedgerError};
