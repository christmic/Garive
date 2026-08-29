//! Tool declarations, prepared calls, neutral outcomes, and capability ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod governed_types;
mod prepared;
mod schema;
mod schema_validate;
mod unique_json;

pub use governed_types::{
    DispatchAttemptId, EffectReceipt, GrantId, InteractionId, InteractionKind, InteractionRequest,
    InteractionResolution, InvocationGrant, ReceiptId, TerminalClassification, ToolInvocationId,
};
pub use prepared::{
    ExecutionCapability, ExecutionRequirements, PreparationError, PreparationErrorCode,
    PreparedToolCall, ReplayClass, SchemaFailure, ToolCatalog, ToolDefinition, ToolIntent,
};
