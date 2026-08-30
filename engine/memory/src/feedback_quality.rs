use crate::{values::valid_digest, MemoryError, MemoryErrorCode, RecallQualityRatio};

const MAX_FEEDBACK_ROWS: usize = 4096;

/// Attributable reality outcome for one applied recalled revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecallFeedbackOutcome {
    /// Reality verified the revision in scope.
    Verified,
    /// Reality falsified the revision in scope.
    Falsified,
    /// Reality was inconclusive or outside scope.
    Neutral,
}

/// Content-free projection of one recall/application/outcome chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallFeedbackRow {
    /// Stable exposure identity; rows must be in lexical order.
    pub exposure_id: String,
    /// Frozen recall selection identity.
    pub selection_id: String,
    /// Stable logical record identity.
    pub record_id: String,
    /// Exact immutable revision identity.
    pub revision_id: String,
    /// Whether a committed application obligation exists.
    pub applied: bool,
    /// Optional attributable reality observation.
    pub outcome: Option<RecallFeedbackOutcome>,
}

/// Version bindings and rows for one exact feedback reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallFeedbackQualityRequest {
    /// Candidate/selection policy revision.
    pub policy_revision: String,
    /// Candidate-port revision.
    pub candidate_port_revision: String,
    /// Attribution policy revision.
    pub attribution_policy_revision: String,
    /// Verifier revision.
    pub verifier_revision: String,
    /// Digest of the fixed prefix or pinned corpus.
    pub corpus_digest: String,
    /// Ordered content-free chain rows.
    pub rows: Vec<RecallFeedbackRow>,
}

/// Exact integer evidence from attributable production or pinned chains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecallFeedbackQualitySummary {
    /// Eligible recall exposures.
    pub exposures: u64,
    /// Exposures with committed application obligations.
    pub applications: u64,
    /// Unapplied exposures, which are not observations.
    pub censored: u64,
    /// Applied rows awaiting an observation.
    pub pending: u64,
    /// Reality-verified outcomes.
    pub verified: u64,
    /// In-scope reality-falsified outcomes.
    pub falsified: u64,
    /// Inconclusive or out-of-scope observations.
    pub neutral: u64,
    /// Applications divided by exposures.
    pub application_ratio: Option<RecallQualityRatio>,
    /// Verified divided by conclusive verified plus falsified outcomes.
    pub verified_outcome_ratio: Option<RecallQualityRatio>,
}

/// Reduces one bounded, version-bound chain set without I/O or floating point.
pub fn evaluate_recall_feedback_quality(
    request: &RecallFeedbackQualityRequest,
) -> Result<RecallFeedbackQualitySummary, MemoryError> {
    if request.rows.len() > MAX_FEEDBACK_ROWS
        || !valid_digest(&request.corpus_digest)
        || [
            &request.policy_revision,
            &request.candidate_port_revision,
            &request.attribution_policy_revision,
            &request.verifier_revision,
        ]
        .into_iter()
        .any(|value| value.is_empty() || value.trim() != value)
        || !request
            .rows
            .windows(2)
            .all(|pair| pair[0].exposure_id < pair[1].exposure_id)
        || request.rows.iter().any(|row| {
            [
                &row.exposure_id,
                &row.selection_id,
                &row.record_id,
                &row.revision_id,
            ]
            .into_iter()
            .any(|value| value.is_empty() || value.trim() != value)
                || (!row.applied && row.outcome.is_some())
        })
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    let applications = request.rows.iter().filter(|row| row.applied).count() as u64;
    let verified = outcomes(request, RecallFeedbackOutcome::Verified);
    let falsified = outcomes(request, RecallFeedbackOutcome::Falsified);
    let neutral = outcomes(request, RecallFeedbackOutcome::Neutral);
    let exposures = request.rows.len() as u64;
    let conclusive = verified + falsified;
    Ok(RecallFeedbackQualitySummary {
        exposures,
        applications,
        censored: exposures - applications,
        pending: request
            .rows
            .iter()
            .filter(|row| row.applied && row.outcome.is_none())
            .count() as u64,
        verified,
        falsified,
        neutral,
        application_ratio: ratio(applications, exposures),
        verified_outcome_ratio: ratio(verified, conclusive),
    })
}

fn outcomes(request: &RecallFeedbackQualityRequest, expected: RecallFeedbackOutcome) -> u64 {
    request
        .rows
        .iter()
        .filter(|row| row.outcome == Some(expected))
        .count() as u64
}

const fn ratio(numerator: u64, denominator: u64) -> Option<RecallQualityRatio> {
    if denominator == 0 {
        None
    } else {
        Some(RecallQualityRatio {
            numerator,
            denominator,
        })
    }
}
