use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Provider-neutral token count that never encodes missing evidence as zero.
pub enum TokenCount {
    /// Count reported or deliberately estimated by an admitted source.
    Known(u64),
    /// The source did not provide a usable count.
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Checked sum of input and output token evidence.
pub enum UsageTotal {
    /// Both components were known and added without overflow.
    Known(u64),
    /// At least one component was unknown.
    Unknown,
    /// Known components exceeded the admitted integer range.
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Provenance of normalized usage evidence.
pub enum UsageSource {
    /// Counts came from the provider response.
    ProviderReported,
    /// Counts came from an explicit conservative Runtime policy.
    Estimated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Normalized token evidence for one model invocation outcome.
pub struct ModelUsage {
    /// Prompt/input tokens, or [`TokenCount::Unknown`].
    pub input_tokens: TokenCount,
    /// Generated/output tokens, or [`TokenCount::Unknown`].
    pub output_tokens: TokenCount,
    /// Optional cache-read breakdown; not added again by [`Self::total_tokens`].
    pub cache_read_tokens: Option<TokenCount>,
    /// Optional cache-write breakdown; not added again by [`Self::total_tokens`].
    pub cache_write_tokens: Option<TokenCount>,
    /// Origin of the normalized counts.
    pub source: UsageSource,
}

impl ModelUsage {
    /// Adds input and output evidence with checked arithmetic.
    pub const fn total_tokens(self) -> UsageTotal {
        match (self.input_tokens, self.output_tokens) {
            (TokenCount::Known(input), TokenCount::Known(output)) => {
                match input.checked_add(output) {
                    Some(total) => UsageTotal::Known(total),
                    None => UsageTotal::Overflow,
                }
            }
            _ => UsageTotal::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Reasoning content admitted for downstream model-neutral handling.
pub enum ReasoningContent {
    /// Reasoning text the provider allows the model/client boundary to expose.
    ModelVisible(String),
    /// Opaque provider reference retained without exposing hidden reasoning.
    OpaqueReference(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Provider-neutral media classification.
pub enum MediaKind {
    /// Still image content.
    Image,
    /// Audio content.
    Audio,
    /// Video content.
    Video,
    /// Generic file content.
    File,
    /// Forward-compatible media class named by an adapter.
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Ordered provider-neutral item produced by or supplied to model workflows.
pub enum ModelItem {
    /// Ordinary generated text.
    Text {
        /// Generated UTF-8 text.
        text: String,
    },
    /// Valid provider-declared refusal, distinct from transport failure.
    Refusal {
        /// Sanitized refusal text.
        text: String,
    },
    /// Model-visible reasoning or an opaque reasoning reference.
    Reasoning {
        /// Admitted reasoning representation.
        content: ReasoningContent,
    },
    /// Untrusted model proposal to call one named tool.
    ToolIntent {
        /// Provider/model-owned correlation identity.
        model_call_id: String,
        /// Requested provider-neutral tool name.
        tool_name: String,
        /// Structured arguments encoded as JSON text.
        arguments_json: String,
    },
    /// Provider-neutral tool result associated with a model call.
    ToolObservation {
        /// Model call identity being answered.
        model_call_id: String,
        /// Structured neutral result encoded as JSON text.
        result_json: String,
    },
    /// Reference to media stored outside the model contract.
    MediaReference {
        /// Media classification.
        media_kind: MediaKind,
        /// Runtime/adaptor-resolvable content reference.
        reference: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Normalized reason a completed provider response stopped generating.
pub enum ModelStopReason {
    /// Provider declared the turn complete.
    EndTurn,
    /// Provider stopped to request tool execution.
    ToolUse,
    /// A configured stop sequence matched.
    StopSequence,
    /// Provider requested a resumable pause.
    PauseTurn,
    /// Provider completed with a refusal.
    Refusal,
    /// Forward-compatible provider-neutral stop classification.
    Other(String),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Valid request rejected before or instead of model execution.
pub enum RejectionKind {
    /// Input exceeded the selected model's context surface.
    ContextOverflow,
    /// Provider authentication or authorization failed.
    Authentication,
    /// Provider content policy rejected the request.
    ContentPolicy,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Invocation began but did not produce a complete response.
pub enum InterruptionKind {
    /// Cooperative cancellation interrupted processing.
    Cancelled,
    /// Provider reached its output bound and may have partial items.
    OutputLimit,
    /// Transport failed after dispatch could have occurred.
    Transport,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Resource state preventing the selected model from being invoked.
pub enum UnavailableKind {
    /// Provider throttled the caller.
    RateLimited,
    /// Requested model resource is temporarily unavailable.
    ModelUnavailable,
    /// Local/adaptor circuit breaker rejected dispatch.
    CircuitOpen,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Exactly one normalized fact envelope returned by [`crate::ModelPort`].
pub enum InvokeOutcome {
    /// Model processing completed with ordered items and usage evidence.
    Completed {
        /// Ordered normalized response items.
        items: Vec<ModelItem>,
        /// Normalized usage evidence.
        usage: ModelUsage,
        /// Provider-neutral completion reason.
        stop_reason: ModelStopReason,
    },
    /// Provider rejected the request without a valid model response.
    Rejected {
        /// Stable rejection classification.
        kind: RejectionKind,
        /// Secret-free evidence safe for durable diagnostics.
        sanitized_evidence: String,
    },
    /// Processing was interrupted and may contain valid partial output.
    Interrupted {
        /// Stable interruption classification.
        kind: InterruptionKind,
        /// Valid normalized items observed before interruption.
        partial_items: Vec<ModelItem>,
        /// Usage evidence available at interruption time.
        usage: ModelUsage,
    },
    /// Model dispatch could not currently proceed.
    Unavailable {
        /// Stable resource classification.
        kind: UnavailableKind,
        /// Optional provider-advised minimum retry delay.
        retry_after: Option<Duration>,
    },
}

impl InvokeOutcome {
    /// Returns the stable top-level outcome class.
    pub const fn kind(&self) -> InvokeOutcomeKind {
        match self {
            Self::Completed { .. } => InvokeOutcomeKind::Completed,
            Self::Rejected { .. } => InvokeOutcomeKind::Rejected,
            Self::Interrupted { .. } => InvokeOutcomeKind::Interrupted,
            Self::Unavailable { .. } => InvokeOutcomeKind::Unavailable,
        }
    }

    /// Returns `true` only for [`InvokeOutcome::Completed`].
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    /// Returns `true` only when valid partial items may be present.
    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::Interrupted { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Field-free classification of [`InvokeOutcome`].
pub enum InvokeOutcomeKind {
    /// Completed outcome.
    Completed,
    /// Rejected outcome.
    Rejected,
    /// Interrupted outcome.
    Interrupted,
    /// Unavailable outcome.
    Unavailable,
}
