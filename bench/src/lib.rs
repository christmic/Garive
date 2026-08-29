//! Thin official SWE benchmark loader, driver, adapters and evaluator boundary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod case;
mod error;
mod unique_json;

pub use case::{parse_cases, CaseLoadLimits, SweCase, SweDataset};
pub use error::{BenchError, BenchErrorCode};
