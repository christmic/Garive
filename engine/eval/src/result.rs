use std::collections::BTreeSet;

use crate::{
    EvaluationCaseId, EvaluationError, EvaluationErrorCode, EvaluationLimits, EvaluationScore,
};

/// Terminal classification of one evaluation case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationCaseOutcome {
    /// The official evaluator resolved the case.
    Passed,
    /// The official evaluator ran and did not resolve the case.
    Failed,
    /// Infrastructure prevented a valid Agent verdict.
    InfrastructureFailure,
    /// The case never entered an attempt.
    NotAttempted,
}

/// Exact terminal evidence for one case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationCaseResult {
    /// Stable case identity.
    pub case_id: EvaluationCaseId,
    /// Terminal classification.
    pub outcome: EvaluationCaseOutcome,
    /// Measured wall duration for this case.
    pub duration_ms: u64,
    /// Known model input tokens, or unknown.
    pub input_tokens: Option<u64>,
    /// Known model output tokens, or unknown.
    pub output_tokens: Option<u64>,
}

/// Deterministic aggregate separating Agent and infrastructure evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationSummary {
    /// Passed plus failed Agent cases.
    pub attempted: u64,
    /// Officially resolved Agent cases.
    pub passed: u64,
    /// Officially unresolved Agent cases.
    pub failed: u64,
    /// Cases without a valid Agent verdict due to infrastructure.
    pub infrastructure_failed: u64,
    /// Cases that never entered an attempt.
    pub not_attempted: u64,
    /// Exact score, absent when no Agent verdict exists.
    pub score: Option<EvaluationScore>,
}

/// Reduces unique terminal results under explicit run limits.
pub fn summarize(
    results: &[EvaluationCaseResult],
    limits: EvaluationLimits,
) -> Result<EvaluationSummary, EvaluationError> {
    let limits = limits.validate()?;
    if results.len() > limits.max_cases {
        return Err(EvaluationError::new(EvaluationErrorCode::TooManyCases));
    }
    let mut ids = BTreeSet::new();
    let mut summary = EvaluationSummary {
        attempted: 0,
        passed: 0,
        failed: 0,
        infrastructure_failed: 0,
        not_attempted: 0,
        score: None,
    };
    for result in results {
        if !ids.insert(result.case_id.clone()) {
            return Err(EvaluationError::new(EvaluationErrorCode::DuplicateCase));
        }
        if result.duration_ms > limits.max_case_duration_ms {
            return Err(EvaluationError::new(EvaluationErrorCode::DurationExceeded));
        }
        match result.outcome {
            EvaluationCaseOutcome::Passed => increment(&mut summary.passed)?,
            EvaluationCaseOutcome::Failed => increment(&mut summary.failed)?,
            EvaluationCaseOutcome::InfrastructureFailure => {
                increment(&mut summary.infrastructure_failed)?
            }
            EvaluationCaseOutcome::NotAttempted => {
                if result.duration_ms != 0
                    || result.input_tokens.is_some()
                    || result.output_tokens.is_some()
                {
                    return Err(EvaluationError::new(
                        EvaluationErrorCode::InvalidNotAttempted,
                    ));
                }
                increment(&mut summary.not_attempted)?;
            }
        }
    }
    summary.attempted = summary
        .passed
        .checked_add(summary.failed)
        .ok_or_else(|| EvaluationError::new(EvaluationErrorCode::CountOverflow))?;
    summary.score = if summary.attempted == 0 {
        None
    } else {
        Some(EvaluationScore::new(summary.passed, summary.attempted)?)
    };
    Ok(summary)
}

fn increment(value: &mut u64) -> Result<(), EvaluationError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| EvaluationError::new(EvaluationErrorCode::CountOverflow))?;
    Ok(())
}
