//! Durable-fact vocabulary and ledger ports; storage adapters live in Runtime.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod canonical;
mod projection;
mod runtime_facts;
mod state;
mod types;

pub use canonical::{CanonicalPayload, CanonicalPayloadError};
pub use runtime_facts::{validate_runtime_fact, RuntimeFactDisposition};
pub use state::{LedgerState, TurnSnapshot};
pub use types::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CommitDisposition, CommitResult,
    DurableFact, ExecutionId, FactDraft, FactId, FactKind, InvalidLedgerIdentity, LedgerError,
    ModelRequestId, SessionId, ToolInvocationId, TurnId,
};
