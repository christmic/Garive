//! Pure evaluation identities, terminal evidence, summaries and baselines.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod baseline;
mod result;
mod values;

pub use baseline::{EvaluationBaseline, EvaluationBaselineProvenance};
pub use result::{summarize, EvaluationCaseOutcome, EvaluationCaseResult, EvaluationSummary};
pub use values::{
    EvaluationCaseId, EvaluationError, EvaluationErrorCode, EvaluationLimits, EvaluationRunId,
    EvaluationScore, EvaluationSuiteId, MAX_EVALUATION_ID_BYTES,
};
