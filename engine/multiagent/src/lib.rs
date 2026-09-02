//! Agent delegation semantics and neutral child-execution ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod budget;
mod collaboration;
mod grant;
mod intent;
mod result;
mod values;

pub use budget::{
    settle_delegation_budget, BudgetAmounts, DelegationBudgetSettlement, DelegationConsumption,
    DelegationUsage, TokenUsageEvidence,
};
pub use collaboration::{
    AssigneeSelector, DeliveryPolicy, NamedAgent, SessionRoster, MAX_NAMED_SESSION_AGENTS,
};
pub use grant::{
    authorize_delegation, release_delegation_budget, DelegationAllowance, DelegationAuthorization,
    DelegationGrant,
};
pub use intent::{CancellationPolicy, ChildRequirement, DelegationIntent, DelegationIntentBinding};
pub use result::{
    complete_delegation_result, terminal_delegation_result, ChildTerminalReason, DelegationOutcome,
    DelegationResult, DelegationResultContext, TerminalOutcomeKind,
};
pub use values::{
    ContentBinding, DelegationBudget, DelegationError, DelegationErrorCode, FactReference,
};
