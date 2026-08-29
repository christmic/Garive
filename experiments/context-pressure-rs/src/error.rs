use std::fmt;

/// Stable C7-A corpus/runner failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextPressureErrorCode {
    /// Input was not one strict schema-v1 JSON document.
    InvalidDocument,
    /// A corpus identity, revision, class set or bound was invalid.
    InvalidCorpus,
    /// A case violated the accepted C2 candidate contract.
    InvalidContext,
    /// An uncompressed case dropped an eligible candidate.
    CompressedInput,
    /// The injected counter descriptor was incomplete or malformed.
    InvalidCounter,
    /// The injected counter could not return exact non-zero evidence.
    CounterFailure,
    /// Pure evidence construction or reduction failed.
    ReductionFailure,
}

/// Content-free C7-A error safe for logs and evidence boundaries.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ContextPressureError {
    code: ContextPressureErrorCode,
}

impl ContextPressureError {
    pub(crate) const fn new(code: ContextPressureErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure classification.
    pub const fn code(self) -> ContextPressureErrorCode {
        self.code
    }
}

impl fmt::Debug for ContextPressureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContextPressureError")
            .field("code", &self.code)
            .finish()
    }
}
