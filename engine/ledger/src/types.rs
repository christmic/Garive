use std::{error::Error, fmt};

use crate::{CanonicalPayload, CanonicalPayloadError};

macro_rules! ledger_identity {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = InvalidLedgerIdentity;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                if value.is_empty() {
                    Err(InvalidLedgerIdentity($label))
                } else {
                    Ok(Self(value.into()))
                }
            }
        }
    };
}

ledger_identity!(SessionId, "session");
ledger_identity!(TurnId, "turn");
ledger_identity!(ExecutionId, "execution");
ledger_identity!(FactId, "fact");
ledger_identity!(ModelRequestId, "model request");
ledger_identity!(ToolInvocationId, "tool invocation");
ledger_identity!(AgentInstanceId, "agent instance");
ledger_identity!(AgentDefinitionId, "agent definition");
ledger_identity!(AgentDefinitionRevision, "agent definition revision");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidLedgerIdentity(&'static str);

impl fmt::Display for InvalidLedgerIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} identity cannot be empty", self.0)
    }
}

impl Error for InvalidLedgerIdentity {}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactKind(Box<str>);

impl FactKind {
    pub fn new(value: impl Into<Box<str>>) -> Result<Self, LedgerError> {
        let value = value.into();
        if value.is_empty() {
            Err(LedgerError::InvalidFact)
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactDraft {
    pub fact_id: FactId,
    pub turn_id: Option<TurnId>,
    pub execution_id: Option<ExecutionId>,
    pub model_request_id: Option<ModelRequestId>,
    pub tool_invocation_id: Option<ToolInvocationId>,
    pub kind: FactKind,
    pub schema_version: u32,
    pub payload: CanonicalPayload,
    pub recorded_at: String,
}

impl FactDraft {
    pub fn same_semantics(&self, other: &Self) -> bool {
        self.fact_id == other.fact_id
            && self.turn_id == other.turn_id
            && self.execution_id == other.execution_id
            && self.model_request_id == other.model_request_id
            && self.tool_invocation_id == other.tool_invocation_id
            && self.kind == other.kind
            && self.schema_version == other.schema_version
            && self.payload == other.payload
    }

    pub fn validate(&self) -> Result<(), LedgerError> {
        if self.schema_version == 0
            || chrono::DateTime::parse_from_rfc3339(&self.recorded_at).is_err()
        {
            return Err(LedgerError::InvalidFact);
        }
        self.payload.verify().map_err(LedgerError::Corruption)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableFact {
    pub fact_id: FactId,
    pub session_id: SessionId,
    pub position: u64,
    pub turn_id: Option<TurnId>,
    pub execution_id: Option<ExecutionId>,
    pub model_request_id: Option<ModelRequestId>,
    pub tool_invocation_id: Option<ToolInvocationId>,
    pub kind: FactKind,
    pub schema_version: u32,
    pub payload: CanonicalPayload,
    pub recorded_at: String,
}

impl DurableFact {
    pub fn verify(&self) -> Result<(), LedgerError> {
        if self.position == 0
            || self.schema_version == 0
            || chrono::DateTime::parse_from_rfc3339(&self.recorded_at).is_err()
        {
            return Err(LedgerError::Corruption(CanonicalPayloadError::InvalidJson));
        }
        self.payload.verify().map_err(LedgerError::Corruption)
    }
}

impl From<(SessionId, u64, FactDraft)> for DurableFact {
    fn from((session_id, position, value): (SessionId, u64, FactDraft)) -> Self {
        Self {
            fact_id: value.fact_id,
            session_id,
            position,
            turn_id: value.turn_id,
            execution_id: value.execution_id,
            model_request_id: value.model_request_id,
            tool_invocation_id: value.tool_invocation_id,
            kind: value.kind,
            schema_version: value.schema_version,
            payload: value.payload,
            recorded_at: value.recorded_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitDisposition {
    Committed,
    Replayed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitResult {
    pub disposition: CommitDisposition,
    pub session_version: u64,
    pub positions: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LedgerError {
    EmptyBatch,
    ConcurrentModification,
    IdempotencyCollision,
    IncompleteReplay,
    InvalidFact,
    InvalidTransition,
    MissingReference,
    PositionOverflow,
    InvalidReadRange,
    Corruption(CanonicalPayloadError),
}

impl LedgerError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyBatch => "empty-batch",
            Self::ConcurrentModification => "concurrent-modification",
            Self::IdempotencyCollision => "idempotency-collision",
            Self::IncompleteReplay => "incomplete-replay",
            Self::InvalidFact => "invalid-fact",
            Self::InvalidTransition => "invalid-transition",
            Self::MissingReference => "missing-reference",
            Self::PositionOverflow => "position-overflow",
            Self::InvalidReadRange => "invalid-read-range",
            Self::Corruption(CanonicalPayloadError::DigestMismatch) => "digest-mismatch",
            Self::Corruption(_) => "corruption",
        }
    }
}
