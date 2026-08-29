//! Pure evaluation identities, terminal evidence, summaries and baselines.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod baseline;
mod context_pressure;
mod creativity;
mod result;
mod values;

pub use baseline::{EvaluationBaseline, EvaluationBaselineProvenance};
pub use context_pressure::{
    summarize_context_pressure, ContextPressureCaseEvidence, ContextPressureClassSummary,
    ContextPressureSummary, ContextWorkloadClass,
};
pub use creativity::{
    summarize_creativity, CreativityAggregate, CreativityArm, CreativityArmEvidence,
    CreativityClassSummary, CreativitySummary, CreativityTaskClass, CreativityTaskPair,
};
pub use result::{summarize, EvaluationCaseOutcome, EvaluationCaseResult, EvaluationSummary};
pub use values::{
    EvaluationCaseId, EvaluationError, EvaluationErrorCode, EvaluationLimits, EvaluationRunId,
    EvaluationScore, EvaluationSuiteId, MAX_EVALUATION_ID_BYTES,
};
