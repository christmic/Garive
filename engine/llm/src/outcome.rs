use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenCount {
    Known(u64),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageTotal {
    Known(u64),
    Unknown,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageSource {
    ProviderReported,
    Estimated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelUsage {
    pub input_tokens: TokenCount,
    pub output_tokens: TokenCount,
    pub cache_read_tokens: Option<TokenCount>,
    pub cache_write_tokens: Option<TokenCount>,
    pub source: UsageSource,
}

impl ModelUsage {
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
pub enum ReasoningContent {
    ModelVisible(String),
    OpaqueReference(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    File,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelItem {
    Text {
        text: String,
    },
    Reasoning {
        content: ReasoningContent,
    },
    ToolIntent {
        model_call_id: String,
        tool_name: String,
        arguments_json: String,
    },
    ToolObservation {
        model_call_id: String,
        result_json: String,
    },
    MediaReference {
        media_kind: MediaKind,
        reference: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelStopReason {
    EndTurn,
    ToolUse,
    Other(String),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RejectionKind {
    ContextOverflow,
    Authentication,
    ContentPolicy,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterruptionKind {
    Cancelled,
    OutputLimit,
    Transport,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnavailableKind {
    RateLimited,
    ModelUnavailable,
    CircuitOpen,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvokeOutcome {
    Completed {
        items: Vec<ModelItem>,
        usage: ModelUsage,
        stop_reason: ModelStopReason,
    },
    Rejected {
        kind: RejectionKind,
        sanitized_evidence: String,
    },
    Interrupted {
        kind: InterruptionKind,
        partial_items: Vec<ModelItem>,
        usage: ModelUsage,
    },
    Unavailable {
        kind: UnavailableKind,
        retry_after: Option<Duration>,
    },
}

impl InvokeOutcome {
    pub const fn kind(&self) -> InvokeOutcomeKind {
        match self {
            Self::Completed { .. } => InvokeOutcomeKind::Completed,
            Self::Rejected { .. } => InvokeOutcomeKind::Rejected,
            Self::Interrupted { .. } => InvokeOutcomeKind::Interrupted,
            Self::Unavailable { .. } => InvokeOutcomeKind::Unavailable,
        }
    }

    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::Interrupted { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvokeOutcomeKind {
    Completed,
    Rejected,
    Interrupted,
    Unavailable,
}
