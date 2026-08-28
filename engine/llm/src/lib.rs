//! Provider-neutral model requests, outcomes, and adapter contracts.

#![forbid(unsafe_code)]

mod outcome;

pub use outcome::{
    InterruptionKind, InvokeOutcome, InvokeOutcomeKind, MediaKind, ModelItem, ModelStopReason,
    ModelUsage, ReasoningContent, RejectionKind, TokenCount, UnavailableKind, UsageSource,
    UsageTotal,
};
