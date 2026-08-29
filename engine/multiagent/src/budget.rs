use serde::Serialize;

use crate::{DelegationBudget, DelegationError, DelegationErrorCode};

/// Known or conservatively unknown child token usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenUsageEvidence {
    /// Exact non-negative token count.
    Known {
        /// Reported or conservatively measured token count.
        value: u64,
    },
    /// No trustworthy token count exists; settlement charges full reservation.
    Unknown,
}

/// Child token evidence used for conservative settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DelegationUsage {
    /// Child input-token evidence.
    pub input_tokens: TokenUsageEvidence,
    /// Child output-token evidence.
    pub output_tokens: TokenUsageEvidence,
}

/// Exact finite child lifecycle consumption.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct DelegationConsumption {
    /// Child Turns created; exactly one in a terminal v1 result.
    pub child_turns: u64,
    /// Child Executions started, including recovery replacements.
    pub child_executions: u64,
    /// Cumulative completed child iterations.
    pub completed_iterations: u64,
    /// Elapsed child lifecycle milliseconds.
    pub elapsed_ms: u64,
}

impl DelegationConsumption {
    /// Validates the v1 child lifecycle shape.
    pub fn validate(&self) -> Result<(), DelegationError> {
        if self.child_turns == 1 && self.child_executions != 0 {
            Ok(())
        } else {
            Err(DelegationError::new(DelegationErrorCode::InvalidDelegation))
        }
    }
}

/// Consumable aggregate dimensions charged or released by one delegation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BudgetAmounts {
    /// Child Turn units.
    pub child_turns: u64,
    /// Child Execution units.
    pub child_executions: u64,
    /// Kernel iteration units.
    pub iterations: u64,
    /// Input-token units.
    pub input_tokens: u64,
    /// Output-token units.
    pub output_tokens: u64,
    /// Elapsed deadline milliseconds.
    pub elapsed_ms: u64,
}

/// Conservative terminal charge and safely releasable unused reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DelegationBudgetSettlement {
    /// Amount permanently charged to the parent aggregate allowance.
    pub charged: BudgetAmounts,
    /// Unused reservation that may be released only after terminal commit.
    pub released: BudgetAmounts,
}

/// Validates consumption and computes a no-budget-creation terminal settlement.
pub fn settle_delegation_budget(
    reservation: &DelegationBudget,
    consumption: DelegationConsumption,
    usage: DelegationUsage,
) -> Result<DelegationBudgetSettlement, DelegationError> {
    reservation.validate()?;
    consumption.validate()?;
    let input_tokens = charged_tokens(usage.input_tokens, reservation.max_input_tokens)?;
    let output_tokens = charged_tokens(usage.output_tokens, reservation.max_output_tokens)?;
    let charged = BudgetAmounts {
        child_turns: consumption.child_turns,
        child_executions: consumption.child_executions,
        iterations: consumption.completed_iterations,
        input_tokens,
        output_tokens,
        elapsed_ms: consumption.elapsed_ms,
    };
    let reserved = BudgetAmounts {
        child_turns: reservation.max_child_turns,
        child_executions: reservation.max_child_executions,
        iterations: reservation.max_iterations,
        input_tokens: reservation.max_input_tokens,
        output_tokens: reservation.max_output_tokens,
        elapsed_ms: reservation.deadline_budget_ms,
    };
    if charged.child_turns > reserved.child_turns
        || charged.child_executions > reserved.child_executions
        || charged.iterations > reserved.iterations
        || charged.input_tokens > reserved.input_tokens
        || charged.output_tokens > reserved.output_tokens
        || charged.elapsed_ms > reserved.elapsed_ms
    {
        return Err(DelegationError::new(DelegationErrorCode::BudgetExhausted));
    }
    Ok(DelegationBudgetSettlement {
        charged,
        released: BudgetAmounts {
            child_turns: reserved.child_turns - charged.child_turns,
            child_executions: reserved.child_executions - charged.child_executions,
            iterations: reserved.iterations - charged.iterations,
            input_tokens: reserved.input_tokens - charged.input_tokens,
            output_tokens: reserved.output_tokens - charged.output_tokens,
            elapsed_ms: reserved.elapsed_ms - charged.elapsed_ms,
        },
    })
}

fn charged_tokens(evidence: TokenUsageEvidence, reservation: u64) -> Result<u64, DelegationError> {
    match evidence {
        TokenUsageEvidence::Known { value } if value <= reservation => Ok(value),
        TokenUsageEvidence::Known { .. } => {
            Err(DelegationError::new(DelegationErrorCode::BudgetExhausted))
        }
        TokenUsageEvidence::Unknown => Ok(reservation),
    }
}
