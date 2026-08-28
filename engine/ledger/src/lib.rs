//! Durable-fact vocabulary and ledger ports; storage adapters live in Runtime.

#![forbid(unsafe_code)]

mod canonical;
mod types;

pub use canonical::{CanonicalPayload, CanonicalPayloadError};
pub use types::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CommitDisposition, CommitResult,
    DurableFact, ExecutionId, FactDraft, FactId, FactKind, InvalidLedgerIdentity, LedgerError,
    ModelRequestId, SessionId, ToolInvocationId, TurnId,
};
