use std::fmt;

/// Stable strict-corpus or experiment failure code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreativityBaselineErrorCode {
    /// Input was empty, oversized or invalid strict JSON.
    InvalidDocument,
    /// Corpus identity, coverage or task bounds were invalid.
    InvalidCorpus,
    /// A generator dependency failed or returned invalid bounded output.
    GeneratorFailure,
    /// An evaluator dependency failed or returned invalid blind coverage.
    EvaluatorFailure,
    /// Pure paired evidence reduction failed.
    ReductionFailure,
    /// A generator/evaluator descriptor was malformed.
    InvalidPort,
}

impl CreativityBaselineErrorCode {
    /// Returns the stable machine-readable failure name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidDocument => "invalid_document",
            Self::InvalidCorpus => "invalid_corpus",
            Self::GeneratorFailure => "generator_failure",
            Self::EvaluatorFailure => "evaluator_failure",
            Self::ReductionFailure => "reduction_failure",
            Self::InvalidPort => "invalid_port",
        }
    }
}

/// Content-free CR-A infrastructure failure.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct CreativityBaselineError {
    code: CreativityBaselineErrorCode,
}

impl CreativityBaselineError {
    pub(crate) const fn new(code: CreativityBaselineErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure code.
    pub const fn code(self) -> CreativityBaselineErrorCode {
        self.code
    }
}

impl fmt::Debug for CreativityBaselineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreativityBaselineError")
            .field("code", &self.code)
            .finish()
    }
}
