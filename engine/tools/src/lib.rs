//! Tool declarations, prepared calls, neutral outcomes, and capability ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod access;
mod basic_tools;
mod batch;
mod governed_outcome;
mod governed_reducer;
mod governed_types;
mod prepared;
mod sandbox;
mod schema;
mod schema_validate;
mod unique_json;

pub use access::{
    AccessMode, AccessNamespace, AccessPolicyEntry, InvocationAccessSet, ResourceAccess,
    ToolAccessPolicyV1, ToolAccessResolver,
};
pub use basic_tools::{
    BuiltinT1Catalogue, T1_ACCESS_RESOLVER_REVISION, T1_APPLY_PATCH, T1_LIST, T1_PROCESS_RUN,
    T1_READ_TEXT, T1_SEARCH_TEXT, T1_TOOL_REVISION,
};
pub use batch::{
    plan_effect_batch, plan_effect_batch_intents, EffectBatchError, EffectBatchErrorCode,
    EffectBatchIntent, EffectBatchLimitsV1, EffectBatchPlanV1, EffectBatchStep,
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
pub use sandbox::{SandboxControl, SandboxRequirementsV1};
pub use schema::{validate_portable_value, validate_portable_value_schema};
