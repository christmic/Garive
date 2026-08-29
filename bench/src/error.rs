use std::fmt;

/// Stable B0 validation, orchestration or infrastructure failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BenchErrorCode {
    /// One explicit limit was zero.
    InvalidLimits,
    /// The complete case input exceeded its bound.
    DocumentTooLarge,
    /// One JSONL record exceeded its bound.
    LineTooLarge,
    /// Input was not duplicate-free exact UTF-8 JSONL.
    InvalidCaseDocument,
    /// More cases than admitted were supplied.
    TooManyCases,
    /// Two records used the same official instance identity.
    DuplicateCase,
    /// A required case value or official identity was invalid.
    InvalidCase,
    /// A test identity was duplicated or appeared in both test sets.
    InvalidTestSet,
    /// A mandatory port failed or returned incompatible evidence.
    InfrastructureFailure,
    /// A patch was not a bounded repository-relative unified diff.
    InvalidPatch,
    /// Official evaluator arguments or report evidence were invalid.
    InvalidEvaluation,
    /// Tracking output could not be constructed without loss.
    InvalidTracking,
    /// Explicit CLI/run configuration was malformed or ambiguous.
    InvalidConfiguration,
}

impl BenchErrorCode {
    /// Returns the stable machine-readable failure name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidLimits => "invalid_limits",
            Self::DocumentTooLarge => "document_too_large",
            Self::LineTooLarge => "line_too_large",
            Self::InvalidCaseDocument => "invalid_case_document",
            Self::TooManyCases => "too_many_cases",
            Self::DuplicateCase => "duplicate_case",
            Self::InvalidCase => "invalid_case",
            Self::InvalidTestSet => "invalid_test_set",
            Self::InfrastructureFailure => "infrastructure_failure",
            Self::InvalidPatch => "invalid_patch",
            Self::InvalidEvaluation => "invalid_evaluation",
            Self::InvalidTracking => "invalid_tracking",
            Self::InvalidConfiguration => "invalid_configuration",
        }
    }
}

/// Secret-free typed benchmark failure.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BenchError {
    code: BenchErrorCode,
}

impl BenchError {
    pub(crate) const fn new(code: BenchErrorCode) -> Self {
        Self { code }
    }

    /// Constructs a sanitized failure returned by an injected benchmark port.
    pub const fn from_port(code: BenchErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure code.
    pub const fn code(self) -> BenchErrorCode {
        self.code
    }
}

impl fmt::Debug for BenchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BenchError")
            .field("code", &self.code)
            .finish()
    }
}
