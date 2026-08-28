use std::{error::Error, fmt};

use crate::{CanonicalPayload, CanonicalPayloadError};

macro_rules! ledger_identity {
    ($name:ident, $label:literal) => {
        #[doc = concat!("Validated, non-empty ", $label, " identity.")]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            #[doc = concat!("Returns the ", $label, " identity as text.")]
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
/// Error returned when a Ledger identity is empty.
pub struct InvalidLedgerIdentity(&'static str);

impl fmt::Display for InvalidLedgerIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} identity cannot be empty", self.0)
    }
}

impl Error for InvalidLedgerIdentity {}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
/// Stable, non-empty semantic name for one durable fact kind.
pub struct FactKind(Box<str>);

impl FactKind {
    /// Validates and constructs a fact kind.
    pub fn new(value: impl Into<Box<str>>) -> Result<Self, LedgerError> {
        let value = value.into();
        if value.is_empty() {
            Err(LedgerError::InvalidFact)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the stable fact kind name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Unpositioned fact supplied by Runtime as part of an atomic commit.
pub struct FactDraft {
    /// Runtime-assigned idempotency identity.
    pub fact_id: FactId,
    /// Owning Turn when the fact belongs to a Turn lifecycle.
    pub turn_id: Option<TurnId>,
    /// Owning Execution when the fact belongs to one bounded execution.
    pub execution_id: Option<ExecutionId>,
    /// Model invocation identity when the fact records a model lifecycle.
    pub model_request_id: Option<ModelRequestId>,
    /// Tool invocation identity when the fact records an external effect.
    pub tool_invocation_id: Option<ToolInvocationId>,
    /// Semantic fact kind interpreted by admitted projections.
    pub kind: FactKind,
    /// Version of the kind-specific payload schema.
    pub schema_version: u32,
    /// Canonical payload and its integrity digest.
    pub payload: CanonicalPayload,
    /// RFC 3339 observation time; durable position remains ordering truth.
    pub recorded_at: String,
}

impl FactDraft {
    /// Compares the idempotency-bound semantic fields, excluding observation time.
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

    /// Validates schema, timestamp, and canonical payload integrity.
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
/// Immutable fact after assignment to a Session-local durable position.
pub struct DurableFact {
    /// Runtime-assigned idempotency identity.
    pub fact_id: FactId,
    /// Session whose ordered stream contains the fact.
    pub session_id: SessionId,
    /// Non-zero, monotonically increasing Session-local replay position.
    pub position: u64,
    /// Owning Turn, when applicable.
    pub turn_id: Option<TurnId>,
    /// Owning bounded Execution, when applicable.
    pub execution_id: Option<ExecutionId>,
    /// Model invocation identity, when applicable.
    pub model_request_id: Option<ModelRequestId>,
    /// Tool invocation identity, when applicable.
    pub tool_invocation_id: Option<ToolInvocationId>,
    /// Semantic fact kind.
    pub kind: FactKind,
    /// Version of the kind-specific payload schema.
    pub schema_version: u32,
    /// Canonical payload and integrity digest.
    pub payload: CanonicalPayload,
    /// RFC 3339 observation time; not used for replay ordering.
    pub recorded_at: String,
}

impl DurableFact {
    /// Verifies durable position, schema, timestamp, and payload integrity.
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
/// Whether a commit appended new facts or replayed an identical prior batch.
pub enum CommitDisposition {
    /// The batch was validated and appended at new positions.
    Committed,
    /// Every fact already existed with identical idempotency-bound semantics.
    Replayed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Durable coordinates returned after a successful commit or replay.
pub struct CommitResult {
    /// Whether the call committed or replayed the batch.
    pub disposition: CommitDisposition,
    /// Session version after the operation.
    pub session_version: u64,
    /// Contiguous durable positions corresponding to input fact order.
    pub positions: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Typed failure from Ledger validation, transition, concurrency, or integrity checks.
pub enum LedgerError {
    /// A commit contained no facts.
    EmptyBatch,
    /// The expected Session version did not match durable state.
    ConcurrentModification,
    /// A Fact ID already exists with different idempotency-bound semantics.
    IdempotencyCollision,
    /// Only part of a submitted batch was found during idempotent replay.
    IncompleteReplay,
    /// A fact envelope or duplicate within a batch is invalid.
    InvalidFact,
    /// The fact would violate an admitted aggregate lifecycle.
    InvalidTransition,
    /// A referenced Turn, Execution, or invocation does not exist.
    MissingReference,
    /// A durable position or version cannot be incremented safely.
    PositionOverflow,
    /// A requested durable-position range is empty or reversed.
    InvalidReadRange,
    /// Persisted canonical payload evidence is invalid.
    Corruption(CanonicalPayloadError),
}

impl LedgerError {
    /// Returns the stable machine-readable error code.
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
