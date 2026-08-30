use std::collections::BTreeSet;

use crate::{EvaluationCaseId, EvaluationError, EvaluationErrorCode};

/// One explicit arm in the paired CR-A experiment.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CreativityArm {
    /// Exactly one generated candidate.
    Control,
    /// Two or more candidates under a frozen bound.
    BoundedAlternatives,
}

impl CreativityArm {
    /// Fixed pair order used in evidence.
    pub const ALL: [Self; 2] = [Self::Control, Self::BoundedAlternatives];

    /// Returns the stable evidence name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::BoundedAlternatives => "bounded_alternatives",
        }
    }
}

/// Neutral task class in the CR-A corpus.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CreativityTaskClass {
    /// Materially different valid designs.
    DesignAlternatives,
    /// Distinct testable explanations.
    DiagnosticHypotheses,
    /// Different plans satisfying competing constraints.
    ConstraintReconciliation,
    /// Distinct representations preserving required meaning.
    TransformationReframing,
}

impl CreativityTaskClass {
    /// Fixed complete corpus/reporting order.
    pub const ALL: [Self; 4] = [
        Self::DesignAlternatives,
        Self::DiagnosticHypotheses,
        Self::ConstraintReconciliation,
        Self::TransformationReframing,
    ];

    /// Returns the stable fixture and evidence name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::DesignAlternatives => "design_alternatives",
            Self::DiagnosticHypotheses => "diagnostic_hypotheses",
            Self::ConstraintReconciliation => "constraint_reconciliation",
            Self::TransformationReframing => "transformation_reframing",
        }
    }
}

/// Exact blind-evaluator result for one task arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityArmEvidence {
    /// Stable task identity.
    pub task_id: EvaluationCaseId,
    /// Neutral task class.
    pub task_class: CreativityTaskClass,
    /// Explicit experiment arm.
    pub arm: CreativityArm,
    /// Number of generated candidates.
    pub candidate_count: u64,
    /// Candidates the evaluator marked correct.
    pub correct_candidate_count: u64,
    /// Distinct evaluator clusters among correct candidates only.
    pub distinct_correct_cluster_count: u64,
    /// Whether the generator-selected candidate was correct.
    pub selected_correct: bool,
}

impl CreativityArmEvidence {
    /// Validates arm shape and correct-only count relations.
    pub fn new(
        task_id: EvaluationCaseId,
        task_class: CreativityTaskClass,
        arm: CreativityArm,
        candidate_count: u64,
        correct_candidate_count: u64,
        distinct_correct_cluster_count: u64,
        selected_correct: bool,
    ) -> Result<Self, EvaluationError> {
        let arm_shape = match arm {
            CreativityArm::Control => candidate_count == 1,
            CreativityArm::BoundedAlternatives => candidate_count >= 2,
        };
        if !arm_shape
            || correct_candidate_count > candidate_count
            || distinct_correct_cluster_count > correct_candidate_count
            || (selected_correct && correct_candidate_count == 0)
        {
            return Err(error(EvaluationErrorCode::InvalidCreativityEvidence));
        }
        Ok(Self {
            task_id,
            task_class,
            arm,
            candidate_count,
            correct_candidate_count,
            distinct_correct_cluster_count,
            selected_correct,
        })
    }
}

/// One complete source-ordered control/treatment pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityTaskPair {
    /// Stable task identity.
    pub task_id: EvaluationCaseId,
    /// Neutral task class.
    pub task_class: CreativityTaskClass,
    /// Exact control evidence.
    pub control: CreativityArmEvidence,
    /// Exact bounded-alternatives evidence.
    pub bounded_alternatives: CreativityArmEvidence,
}

/// Checked exact totals and rational means for one arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityAggregate {
    /// Aggregated arm.
    pub arm: CreativityArm,
    /// Number of paired tasks.
    pub task_count: u64,
    /// Total candidates.
    pub candidate_count: u64,
    /// Total correct candidates.
    pub correct_candidate_count: u64,
    /// Sum of per-task distinct correct cluster counts.
    pub correct_cluster_mean_numerator: u64,
    /// Task count forming the exact cluster-mean denominator.
    pub correct_cluster_mean_denominator: u64,
    /// Correct selected answers forming an exact rate numerator.
    pub selected_correct_numerator: u64,
    /// Task count forming the selected-correctness denominator.
    pub selected_correct_denominator: u64,
}

/// One class-specific paired reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityClassSummary {
    /// Neutral task class.
    pub task_class: CreativityTaskClass,
    /// Control totals for this class.
    pub control: CreativityAggregate,
    /// Bounded-alternative totals for this class.
    pub bounded_alternatives: CreativityAggregate,
}

/// Complete source-order-preserving CR-A reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativitySummary {
    /// Complete task pairs in corpus order.
    pub ordered_pairs: Vec<CreativityTaskPair>,
    /// Control totals across every task.
    pub control: CreativityAggregate,
    /// Bounded-alternative totals across every task.
    pub bounded_alternatives: CreativityAggregate,
    /// Complete class summaries in [`CreativityTaskClass::ALL`] order.
    pub classes: Vec<CreativityClassSummary>,
}

/// Reduces complete adjacent control/treatment evidence without a composite score.
pub fn summarize_creativity(
    evidence: &[CreativityArmEvidence],
    expected_tasks: usize,
) -> Result<CreativitySummary, EvaluationError> {
    if expected_tasks == 0
        || evidence.len()
            != expected_tasks
                .checked_mul(2)
                .ok_or_else(|| error(EvaluationErrorCode::CreativityArithmeticOverflow))?
    {
        return Err(error(EvaluationErrorCode::MissingCreativityEvidence));
    }
    let mut identities = BTreeSet::new();
    let mut pairs = Vec::with_capacity(expected_tasks);
    for [control, alternatives] in evidence.as_chunks::<2>().0 {
        if control.arm != CreativityArm::Control
            || alternatives.arm != CreativityArm::BoundedAlternatives
            || control.task_id != alternatives.task_id
            || control.task_class != alternatives.task_class
            || !identities.insert(control.task_id.as_str())
        {
            return Err(error(EvaluationErrorCode::InvalidCreativityEvidence));
        }
        pairs.push(CreativityTaskPair {
            task_id: control.task_id.clone(),
            task_class: control.task_class,
            control: control.clone(),
            bounded_alternatives: alternatives.clone(),
        });
    }
    let control = aggregate(
        CreativityArm::Control,
        pairs.iter().map(|pair| &pair.control),
    )?;
    let bounded_alternatives = aggregate(
        CreativityArm::BoundedAlternatives,
        pairs.iter().map(|pair| &pair.bounded_alternatives),
    )?;
    let mut classes = Vec::with_capacity(CreativityTaskClass::ALL.len());
    for task_class in CreativityTaskClass::ALL {
        let selected = pairs.iter().filter(|pair| pair.task_class == task_class);
        let control = aggregate(
            CreativityArm::Control,
            selected.clone().map(|pair| &pair.control),
        )?;
        if control.task_count == 0 {
            return Err(error(EvaluationErrorCode::MissingCreativityEvidence));
        }
        classes.push(CreativityClassSummary {
            task_class,
            control,
            bounded_alternatives: aggregate(
                CreativityArm::BoundedAlternatives,
                selected.map(|pair| &pair.bounded_alternatives),
            )?,
        });
    }
    Ok(CreativitySummary {
        ordered_pairs: pairs,
        control,
        bounded_alternatives,
        classes,
    })
}

fn aggregate<'a>(
    arm: CreativityArm,
    values: impl Iterator<Item = &'a CreativityArmEvidence>,
) -> Result<CreativityAggregate, EvaluationError> {
    let mut result = CreativityAggregate {
        arm,
        task_count: 0,
        candidate_count: 0,
        correct_candidate_count: 0,
        correct_cluster_mean_numerator: 0,
        correct_cluster_mean_denominator: 0,
        selected_correct_numerator: 0,
        selected_correct_denominator: 0,
    };
    for value in values {
        if value.arm != arm {
            return Err(error(EvaluationErrorCode::InvalidCreativityEvidence));
        }
        result.task_count = add(result.task_count, 1)?;
        result.candidate_count = add(result.candidate_count, value.candidate_count)?;
        result.correct_candidate_count = add(
            result.correct_candidate_count,
            value.correct_candidate_count,
        )?;
        result.correct_cluster_mean_numerator = add(
            result.correct_cluster_mean_numerator,
            value.distinct_correct_cluster_count,
        )?;
        result.selected_correct_numerator = add(
            result.selected_correct_numerator,
            u64::from(value.selected_correct),
        )?;
    }
    result.correct_cluster_mean_denominator = result.task_count;
    result.selected_correct_denominator = result.task_count;
    Ok(result)
}

fn add(left: u64, right: u64) -> Result<u64, EvaluationError> {
    left.checked_add(right)
        .ok_or_else(|| error(EvaluationErrorCode::CreativityArithmeticOverflow))
}

const fn error(code: EvaluationErrorCode) -> EvaluationError {
    EvaluationError::new(code)
}
