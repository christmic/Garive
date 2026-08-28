//! Provider-neutral model requests, outcomes, and adapter contracts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod outcome;
mod request;
mod stream;

pub use outcome::{
    InterruptionKind, InvokeOutcome, InvokeOutcomeKind, MediaKind, ModelItem, ModelStopReason,
    ModelUsage, ReasoningContent, RejectionKind, TokenCount, UnavailableKind, UsageSource,
    UsageTotal,
};
pub use request::{
    ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings, ModelRequest,
    ModelRequestId, ModelRole, ModelTargetId, RequestValidationError, TextMode, ToolDescriptor,
};
pub use stream::{
    ModelCancellation, ModelFuture, ModelObserver, ModelOutputKind, ModelPort, ModelPortFailure,
    ModelStreamEvent, ObserverDecision, StreamInvariantError, StreamValidator,
};
