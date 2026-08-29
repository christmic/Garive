//! Reproducible C7-A corpus loading and context-pressure evidence tooling.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod corpus;
mod error;

pub use corpus::{load_corpus, ContextPressureCase, ContextPressureCorpus};
pub use error::{ContextPressureError, ContextPressureErrorCode};
