//! Reproducible paired evidence tooling for the CR-A creativity prerequisite.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod corpus;
mod error;

pub use corpus::{load_creativity_corpus, CreativityCorpus, CreativityTask};
pub use error::{CreativityBaselineError, CreativityBaselineErrorCode};
