//! Tool declarations, prepared calls, neutral outcomes, and capability ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod access;
mod governed_outcome;
mod governed_reducer;
mod governed_types;
mod prepared;
mod schema;
mod schema_validate;
mod unique_json;

pub use access::{
    AccessMode, AccessNamespace, AccessPolicyEntry, InvocationAccessSet, ResourceAccess,
    ToolAccessPolicyV1, ToolAccessResolver,
};
pub use governed_outcome::{
    EffectState, ExecutionFact, GovernedAction, GovernedEffectFailure, GovernedFailureCode,
    GovernedObservation, GovernedToolResult, ObservationOutcome, PreparationRejectedFeedback,
    RecoveryDecision, RecoveryPosition, SuspensionRequirement, ToolFeedback,
};
pub use governed_reducer::{
    recover_effect, reduce_preparation_failure, AuthorizationVerdict, GovernedEffect,
};
pub use governed_types::{
    DispatchAttemptId, EffectReceipt, GrantId, InteractionId, InteractionKind, InteractionRequest,
    InteractionResolution, InvocationGrant, ReceiptId, TerminalClassification, ToolInvocationId,
};
pub use prepared::{
    ExecutionCapability, ExecutionRequirements, PreparationError, PreparationErrorCode,
    PreparedToolCall, ReplayClass, SchemaFailure, ToolCatalog, ToolDefinition, ToolIntent,
};
pub use schema::{validate_portable_value, validate_portable_value_schema};
