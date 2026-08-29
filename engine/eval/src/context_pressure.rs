use std::collections::BTreeSet;

use crate::{EvaluationCaseId, EvaluationError, EvaluationErrorCode};

/// Stable reference workload class required by the C7-A corpus.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextWorkloadClass {
    /// Ordinary multi-turn conversation history.
    Conversation,
    /// Context dominated by governed tool observations.
    ToolHeavy,
    /// Context containing Skill, Memory and Knowledge products.
    CapabilityHeavy,
    /// A long-running session with repeated durable history.
    LongRunning,
}

impl ContextWorkloadClass {
    /// Fixed reporting order for complete baselines.
    pub const ALL: [Self; 4] = [
        Self::Conversation,
        Self::ToolHeavy,
        Self::CapabilityHeavy,
        Self::LongRunning,
    ];

    /// Returns the stable fixture and evidence name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::ToolHeavy => "tool_heavy",
            Self::CapabilityHeavy => "capability_heavy",
            Self::LongRunning => "long_running",
        }
    }
}

/// Exact successful uncompressed measurement for one reference case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPressureCaseEvidence {
    /// Stable case identity.
    pub case_id: EvaluationCaseId,
    /// Reference workload category.
    pub workload_class: ContextWorkloadClass,
    /// Exact retained model input item count.
    pub item_count: usize,
    /// Exact C2 UTF-8 byte cost.
    pub utf8_bytes: usize,
    /// Exact injected counter result.
    pub input_tokens: u64,
    /// Frozen model input limit used for pressure calculation.
    pub model_input_limit_tokens: u64,
    /// Ceiling pressure ratio in basis points; values may exceed 10,000.
    pub pressure_basis_points: u64,
}

impl ContextPressureCaseEvidence {
    /// Validates exact non-zero measurements and calculates checked pressure.
    pub fn new(
        case_id: EvaluationCaseId,
        workload_class: ContextWorkloadClass,
        item_count: usize,
        utf8_bytes: usize,
        input_tokens: u64,
        model_input_limit_tokens: u64,
    ) -> Result<Self, EvaluationError> {
        if item_count == 0 || utf8_bytes == 0 || input_tokens == 0 || model_input_limit_tokens == 0
        {
            return Err(EvaluationError::new(
                EvaluationErrorCode::InvalidPressureEvidence,
            ));
        }
        let scaled = input_tokens
            .checked_mul(10_000)
            .ok_or_else(|| EvaluationError::new(EvaluationErrorCode::PressureArithmeticOverflow))?;
        let pressure_basis_points = scaled
            .checked_add(model_input_limit_tokens - 1)
            .ok_or_else(|| EvaluationError::new(EvaluationErrorCode::PressureArithmeticOverflow))?
            / model_input_limit_tokens;
        Ok(Self {
            case_id,
            workload_class,
            item_count,
            utf8_bytes,
            input_tokens,
            model_input_limit_tokens,
            pressure_basis_points,
        })
    }
}

/// Exact per-class reduction without floating-point values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPressureClassSummary {
    /// Workload category.
    pub workload_class: ContextWorkloadClass,
    /// Number of measured cases.
    pub case_count: u64,
    /// Maximum observed pressure in basis points.
    pub max_pressure_basis_points: u64,
    /// Sum forming the numerator of the exact mean pressure.
    pub mean_pressure_numerator: u64,
    /// Case count forming the denominator of the exact mean pressure.
    pub mean_pressure_denominator: u64,
}

/// Complete source-order-preserving C7-A reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPressureSummary {
    /// Exact input evidence in corpus order.
    pub ordered_cases: Vec<ContextPressureCaseEvidence>,
    /// Four workload summaries in [`ContextWorkloadClass::ALL`] order.
    pub classes: Vec<ContextPressureClassSummary>,
}

/// Reduces a complete bounded C7-A reference run.
pub fn summarize_context_pressure(
    cases: &[ContextPressureCaseEvidence],
    max_cases: usize,
) -> Result<ContextPressureSummary, EvaluationError> {
    if max_cases == 0 || cases.is_empty() || cases.len() > max_cases {
        return Err(EvaluationError::new(
            EvaluationErrorCode::InvalidPressureEvidence,
        ));
    }
    let mut identities = BTreeSet::new();
    for value in cases {
        if !identities.insert(value.case_id.as_str()) {
            return Err(EvaluationError::new(EvaluationErrorCode::DuplicateCase));
        }
    }
    let mut classes = Vec::with_capacity(ContextWorkloadClass::ALL.len());
    for workload_class in ContextWorkloadClass::ALL {
        let selected = cases
            .iter()
            .filter(|value| value.workload_class == workload_class);
        let mut count = 0_u64;
        let mut total = 0_u64;
        let mut maximum = 0_u64;
        for value in selected {
            count = count.checked_add(1).ok_or_else(|| {
                EvaluationError::new(EvaluationErrorCode::PressureArithmeticOverflow)
            })?;
            total = total
                .checked_add(value.pressure_basis_points)
                .ok_or_else(|| {
                    EvaluationError::new(EvaluationErrorCode::PressureArithmeticOverflow)
                })?;
            maximum = maximum.max(value.pressure_basis_points);
        }
        if count == 0 {
            return Err(EvaluationError::new(
                EvaluationErrorCode::MissingWorkloadClass,
            ));
        }
        classes.push(ContextPressureClassSummary {
            workload_class,
            case_count: count,
            max_pressure_basis_points: maximum,
            mean_pressure_numerator: total,
            mean_pressure_denominator: count,
        });
    }
    Ok(ContextPressureSummary {
        ordered_cases: cases.to_vec(),
        classes,
    })
}
