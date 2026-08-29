use std::collections::BTreeSet;

use crate::{MemoryError, MemoryErrorCode};

/// Exact logical Memory revision identity used by pinned evaluation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RecallQualityIdentity {
    /// Stable record identity.
    pub record_id: String,
    /// Exact revision identity.
    pub revision_id: String,
}

/// One pinned recall result with relevance and safety labels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallQualityCase {
    /// Stable dataset-local case identity.
    pub case_id: String,
    /// Ground-truth relevant identities.
    pub expected: Vec<RecallQualityIdentity>,
    /// Identities that must never be selected.
    pub forbidden: Vec<RecallQualityIdentity>,
    /// First deterministic selection.
    pub selected: Vec<RecallQualityIdentity>,
    /// Replayed selection under identical frozen inputs.
    pub replay: Vec<RecallQualityIdentity>,
}

/// Exact unreduced fraction; zero denominators are represented as `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecallQualityRatio {
    /// Matching items.
    pub numerator: u64,
    /// Total eligible items.
    pub denominator: u64,
}

/// Deterministic aggregate for a pinned semantic recall suite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecallQualitySummary {
    /// Valid unique cases consumed.
    pub cases: u64,
    /// Aggregate relevant-item recall.
    pub recall: Option<RecallQualityRatio>,
    /// Aggregate selected-item precision.
    pub precision: Option<RecallQualityRatio>,
    /// Selected identities explicitly labeled forbidden.
    pub forbidden_admissions: u64,
    /// Cases whose identical replay changed order or membership.
    pub replay_mismatches: u64,
}

/// Reduces a finite pinned recall suite without model, storage, or network I/O.
pub fn evaluate_recall_quality(
    cases: &[RecallQualityCase],
) -> Result<RecallQualitySummary, MemoryError> {
    let mut case_ids = BTreeSet::new();
    let mut relevant = 0_u64;
    let mut expected = 0_u64;
    let mut selected = 0_u64;
    let mut forbidden = 0_u64;
    let mut replay_mismatches = 0_u64;
    for case in cases {
        if case.case_id.is_empty()
            || !case_ids.insert(case.case_id.as_str())
            || !unique(&case.expected)
            || !unique(&case.forbidden)
            || !unique(&case.selected)
            || !unique(&case.replay)
            || case
                .expected
                .iter()
                .any(|value| case.forbidden.contains(value))
            || !case.expected.iter().all(valid_identity)
            || !case.forbidden.iter().all(valid_identity)
            || !case.selected.iter().all(valid_identity)
            || !case.replay.iter().all(valid_identity)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        expected = add(expected, case.expected.len())?;
        selected = add(selected, case.selected.len())?;
        relevant = add(
            relevant,
            case.selected
                .iter()
                .filter(|value| case.expected.contains(value))
                .count(),
        )?;
        forbidden = add(
            forbidden,
            case.selected
                .iter()
                .filter(|value| case.forbidden.contains(value))
                .count(),
        )?;
        if case.selected != case.replay {
            replay_mismatches = replay_mismatches
                .checked_add(1)
                .ok_or_else(|| MemoryError::new(MemoryErrorCode::InvalidMemory))?;
        }
    }
    Ok(RecallQualitySummary {
        cases: cases.len() as u64,
        recall: ratio(relevant, expected),
        precision: ratio(relevant, selected),
        forbidden_admissions: forbidden,
        replay_mismatches,
    })
}

fn valid_identity(value: &RecallQualityIdentity) -> bool {
    !value.record_id.is_empty() && !value.revision_id.is_empty()
}

fn unique(values: &[RecallQualityIdentity]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn add(value: u64, amount: usize) -> Result<u64, MemoryError> {
    value
        .checked_add(amount as u64)
        .ok_or_else(|| MemoryError::new(MemoryErrorCode::InvalidMemory))
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
