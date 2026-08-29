use crate::values::valid_id;
use crate::{
    DelegationBudget, DelegationBudgetSettlement, DelegationError, DelegationErrorCode,
    DelegationIntent,
};

/// Parent aggregate remainder and policy caps available for one reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationAllowance {
    /// Remaining child Turn units.
    pub remaining_child_turns: u64,
    /// Remaining child Execution units.
    pub remaining_child_executions: u64,
    /// Remaining child iteration units.
    pub remaining_iterations: u64,
    /// Remaining child input-token units.
    pub remaining_input_tokens: u64,
    /// Remaining child output-token units.
    pub remaining_output_tokens: u64,
    /// Remaining elapsed child milliseconds.
    pub remaining_elapsed_ms: u64,
    /// Highest allowed delegation depth.
    pub max_depth: u64,
    /// Largest objective admitted by parent policy.
    pub max_objective_bytes: u64,
    /// Largest input-evidence count admitted by parent policy.
    pub max_input_evidence: u64,
    /// Largest result-schema byte length admitted by parent policy.
    pub max_result_schema_bytes: u64,
    /// Largest result byte length admitted by parent policy.
    pub max_result_bytes: u64,
    /// Largest result-evidence count admitted by parent policy.
    pub max_result_evidence: u64,
}

/// Exact authority grant committed before child allocation/start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationGrant {
    grant_id: String,
    intent_digest: String,
    reserved_budget: DelegationBudget,
    authority_revision: String,
}

impl DelegationGrant {
    /// Returns stable grant identity.
    pub fn grant_id(&self) -> &str {
        &self.grant_id
    }
    /// Returns exact authorized intent digest.
    pub fn intent_digest(&self) -> &str {
        &self.intent_digest
    }
    /// Returns the fully reserved requested budget.
    pub const fn reserved_budget(&self) -> &DelegationBudget {
        &self.reserved_budget
    }
    /// Returns current Runtime authority revision.
    pub fn authority_revision(&self) -> &str {
        &self.authority_revision
    }
}

/// Pure authorization result with the post-reservation parent remainder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationAuthorization {
    /// Exact grant safe to commit before child work.
    pub grant: DelegationGrant,
    /// Aggregate allowance after atomically reserving the maximum child budget.
    pub remaining: DelegationAllowance,
}

/// Checks depth/concurrency/caps and atomically reserves the requested maximum.
pub fn authorize_delegation(
    intent: &DelegationIntent,
    grant_id: impl Into<String>,
    authority_revision: impl Into<String>,
    current_depth: u64,
    active_parent_delegations: u64,
    allowance: &DelegationAllowance,
) -> Result<DelegationAuthorization, DelegationError> {
    let grant_id = grant_id.into();
    let authority_revision = authority_revision.into();
    if !valid_id(&grant_id) || !valid_id(&authority_revision) {
        return Err(error(DelegationErrorCode::InvalidDelegation));
    }
    if current_depth >= intent.budget().max_depth || current_depth >= allowance.max_depth {
        return Err(error(DelegationErrorCode::DepthExceeded));
    }
    if active_parent_delegations != 0 {
        return Err(error(DelegationErrorCode::ConcurrencyExceeded));
    }
    let requested = intent.budget();
    if requested.max_objective_bytes > allowance.max_objective_bytes
        || requested.max_input_evidence > allowance.max_input_evidence
        || requested.max_result_schema_bytes > allowance.max_result_schema_bytes
        || requested.max_result_bytes > allowance.max_result_bytes
        || requested.max_result_evidence > allowance.max_result_evidence
    {
        return Err(error(DelegationErrorCode::BudgetExhausted));
    }
    let remaining = DelegationAllowance {
        remaining_child_turns: subtract(
            allowance.remaining_child_turns,
            requested.max_child_turns,
        )?,
        remaining_child_executions: subtract(
            allowance.remaining_child_executions,
            requested.max_child_executions,
        )?,
        remaining_iterations: subtract(allowance.remaining_iterations, requested.max_iterations)?,
        remaining_input_tokens: subtract(
            allowance.remaining_input_tokens,
            requested.max_input_tokens,
        )?,
        remaining_output_tokens: subtract(
            allowance.remaining_output_tokens,
            requested.max_output_tokens,
        )?,
        remaining_elapsed_ms: subtract(
            allowance.remaining_elapsed_ms,
            requested.deadline_budget_ms,
        )?,
        max_depth: allowance.max_depth,
        max_objective_bytes: allowance.max_objective_bytes,
        max_input_evidence: allowance.max_input_evidence,
        max_result_schema_bytes: allowance.max_result_schema_bytes,
        max_result_bytes: allowance.max_result_bytes,
        max_result_evidence: allowance.max_result_evidence,
    };
    Ok(DelegationAuthorization {
        grant: DelegationGrant {
            grant_id,
            intent_digest: intent.intent_digest()?,
            reserved_budget: requested.clone(),
            authority_revision,
        },
        remaining,
    })
}

/// Releases only the unused terminal reservation without exceeding the pre-reservation ceiling.
pub fn release_delegation_budget(
    remaining: &DelegationAllowance,
    settlement: DelegationBudgetSettlement,
    ceiling: &DelegationAllowance,
) -> Result<DelegationAllowance, DelegationError> {
    let released = settlement.released;
    let output = DelegationAllowance {
        remaining_child_turns: add(remaining.remaining_child_turns, released.child_turns)?,
        remaining_child_executions: add(
            remaining.remaining_child_executions,
            released.child_executions,
        )?,
        remaining_iterations: add(remaining.remaining_iterations, released.iterations)?,
        remaining_input_tokens: add(remaining.remaining_input_tokens, released.input_tokens)?,
        remaining_output_tokens: add(remaining.remaining_output_tokens, released.output_tokens)?,
        remaining_elapsed_ms: add(remaining.remaining_elapsed_ms, released.elapsed_ms)?,
        max_depth: remaining.max_depth,
        max_objective_bytes: remaining.max_objective_bytes,
        max_input_evidence: remaining.max_input_evidence,
        max_result_schema_bytes: remaining.max_result_schema_bytes,
        max_result_bytes: remaining.max_result_bytes,
        max_result_evidence: remaining.max_result_evidence,
    };
    if output.remaining_child_turns > ceiling.remaining_child_turns
        || output.remaining_child_executions > ceiling.remaining_child_executions
        || output.remaining_iterations > ceiling.remaining_iterations
        || output.remaining_input_tokens > ceiling.remaining_input_tokens
        || output.remaining_output_tokens > ceiling.remaining_output_tokens
        || output.remaining_elapsed_ms > ceiling.remaining_elapsed_ms
    {
        return Err(error(DelegationErrorCode::CorruptDelegationState));
    }
    Ok(output)
}

fn subtract(available: u64, requested: u64) -> Result<u64, DelegationError> {
    available
        .checked_sub(requested)
        .ok_or_else(|| error(DelegationErrorCode::BudgetExhausted))
}

fn add(left: u64, right: u64) -> Result<u64, DelegationError> {
    left.checked_add(right)
        .ok_or_else(|| error(DelegationErrorCode::BudgetOverflow))
}

const fn error(code: DelegationErrorCode) -> DelegationError {
    DelegationError::new(code)
}
