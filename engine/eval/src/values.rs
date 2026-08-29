use std::fmt;

/// Maximum UTF-8 bytes in any stable evaluation identity.
pub const MAX_EVALUATION_ID_BYTES: usize = 256;

macro_rules! identity {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            #[doc = "Validates and owns one bounded non-empty identity."]
            pub fn new(value: impl Into<String>) -> Result<Self, EvaluationError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(EvaluationError::new(EvaluationErrorCode::EmptyIdentity));
                }
                if value.len() > MAX_EVALUATION_ID_BYTES {
                    return Err(EvaluationError::new(EvaluationErrorCode::IdentityTooLong));
                }
                Ok(Self(value))
            }

            #[doc = "Returns the exact identity text."]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

identity!(EvaluationCaseId, "Stable identity of one evaluation case.");
identity!(
    EvaluationSuiteId,
    "Stable identity of one evaluation suite."
);
identity!(EvaluationRunId, "Stable identity of one evaluation run.");

/// Explicit non-zero reduction limits supplied by the evaluation run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationLimits {
    /// Maximum terminal case results admitted to one summary.
    pub max_cases: usize,
    /// Inclusive maximum duration of one case.
    pub max_case_duration_ms: u64,
}

impl EvaluationLimits {
    /// Validates non-zero run bounds.
    pub fn validate(self) -> Result<Self, EvaluationError> {
        if self.max_cases == 0 || self.max_case_duration_ms == 0 {
            Err(EvaluationError::new(EvaluationErrorCode::InvalidLimits))
        } else {
            Ok(self)
        }
    }
}

/// Exact rational evaluation score without floating-point drift.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationScore {
    /// Passed Agent cases.
    pub numerator: u64,
    /// Passed plus failed Agent cases.
    pub denominator: u64,
}

impl EvaluationScore {
    /// Constructs one score with a non-zero denominator and bounded numerator.
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, EvaluationError> {
        if denominator == 0 || numerator > denominator {
            return Err(EvaluationError::new(EvaluationErrorCode::InvalidScore));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

/// Stable evaluation validation/reduction failure code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationErrorCode {
    /// An identity was empty.
    EmptyIdentity,
    /// An identity exceeded its byte bound.
    IdentityTooLong,
    /// One run limit was zero.
    InvalidLimits,
    /// More results than the run bound were supplied.
    TooManyCases,
    /// Two terminal results named the same case.
    DuplicateCase,
    /// One duration exceeded the explicit run bound.
    DurationExceeded,
    /// A not-attempted result claimed duration or usage.
    InvalidNotAttempted,
    /// Checked summary arithmetic overflowed.
    CountOverflow,
    /// A rational score was impossible.
    InvalidScore,
    /// Baseline provenance or digest was invalid.
    InvalidBaseline,
    /// A C7-A measurement or bound was zero or incomplete.
    InvalidPressureEvidence,
    /// A required reference workload class had no case.
    MissingWorkloadClass,
    /// Checked context-pressure arithmetic overflowed.
    PressureArithmeticOverflow,
    /// One creativity arm measurement was impossible or out of order.
    InvalidCreativityEvidence,
    /// A task, arm or required creativity class was missing.
    MissingCreativityEvidence,
    /// Checked creativity reduction arithmetic overflowed.
    CreativityArithmeticOverflow,
}

impl EvaluationErrorCode {
    /// Returns the stable machine-readable failure name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::EmptyIdentity => "empty_identity",
            Self::IdentityTooLong => "identity_too_long",
            Self::InvalidLimits => "invalid_limits",
            Self::TooManyCases => "too_many_cases",
            Self::DuplicateCase => "duplicate_case",
            Self::DurationExceeded => "duration_exceeded",
            Self::InvalidNotAttempted => "invalid_not_attempted",
            Self::CountOverflow => "count_overflow",
            Self::InvalidScore => "invalid_score",
            Self::InvalidBaseline => "invalid_baseline",
            Self::InvalidPressureEvidence => "invalid_pressure_evidence",
            Self::MissingWorkloadClass => "missing_workload_class",
            Self::PressureArithmeticOverflow => "pressure_arithmetic_overflow",
            Self::InvalidCreativityEvidence => "invalid_creativity_evidence",
            Self::MissingCreativityEvidence => "missing_creativity_evidence",
            Self::CreativityArithmeticOverflow => "creativity_arithmetic_overflow",
        }
    }
}

/// Secret-free typed E0 failure.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct EvaluationError {
    code: EvaluationErrorCode,
}

impl EvaluationError {
    pub(crate) const fn new(code: EvaluationErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure code.
    pub const fn code(self) -> EvaluationErrorCode {
        self.code
    }
}

impl fmt::Debug for EvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EvaluationError")
            .field("code", &self.code)
            .finish()
    }
}
