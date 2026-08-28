use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl ModelUsage {
    /// Input plus output. Cache fields are breakdowns, not extra tokens.
    pub const fn total_tokens(self) -> Option<u64> {
        self.input_tokens.checked_add(self.output_tokens)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completed {
    pub text: String,
    pub usage: ModelUsage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverflowEvidence {
    pub normalized_input_tokens: Option<u64>,
    pub accepted_limit_tokens: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartialOutput {
    pub text: String,
    pub usage: ModelUsage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvokeOutcome {
    Completed(Completed),
    Overflow(OverflowEvidence),
    OutputTruncated(PartialOutput),
    RateBudgetExhausted {
        retry_after: Option<Duration>,
    },
    PartialCancelled(PartialOutput),
    AuthFailure {
        reason: String,
    },
    ContentViolation {
        reason: String,
        violated_field: Option<String>,
    },
    ModelUnavailable {
        model_id: String,
    },
    CircuitBreakerOpen {
        target: String,
    },
}

impl InvokeOutcome {
    pub const fn kind(&self) -> InvokeOutcomeKind {
        match self {
            Self::Completed(_) => InvokeOutcomeKind::Completed,
            Self::Overflow(_) => InvokeOutcomeKind::Overflow,
            Self::OutputTruncated(_) => InvokeOutcomeKind::OutputTruncated,
            Self::RateBudgetExhausted { .. } => InvokeOutcomeKind::RateBudgetExhausted,
            Self::PartialCancelled(_) => InvokeOutcomeKind::PartialCancelled,
            Self::AuthFailure { .. } => InvokeOutcomeKind::AuthFailure,
            Self::ContentViolation { .. } => InvokeOutcomeKind::ContentViolation,
            Self::ModelUnavailable { .. } => InvokeOutcomeKind::ModelUnavailable,
            Self::CircuitBreakerOpen { .. } => InvokeOutcomeKind::CircuitBreakerOpen,
        }
    }

    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvokeOutcomeKind {
    Completed,
    Overflow,
    OutputTruncated,
    RateBudgetExhausted,
    PartialCancelled,
    AuthFailure,
    ContentViolation,
    ModelUnavailable,
    CircuitBreakerOpen,
}
