//! Provider-neutral model requests, outcomes, and adapter contracts.

#![forbid(unsafe_code)]

mod outcome;

pub use outcome::{
    Completed, InvokeOutcome, InvokeOutcomeKind, ModelUsage, OverflowEvidence, PartialOutput,
};
