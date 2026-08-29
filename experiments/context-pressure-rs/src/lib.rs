//! Reproducible C7-A corpus loading and context-pressure evidence tooling.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod corpus;
mod counter;
mod error;
mod runner;

pub use corpus::{load_corpus, ContextPressureCase, ContextPressureCorpus};
pub use counter::{TokenCounter, TokenCounterDescriptor, TokenCounterFailure};
pub use error::{ContextPressureError, ContextPressureErrorCode};
pub use runner::{measure_context_pressure, ContextPressureRun};
