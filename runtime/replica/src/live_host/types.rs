use std::{error::Error, fmt, path::PathBuf, sync::Arc};

use garive_ledger::{ExecutionId, SessionId, TurnId};
use serde::{Deserialize, Serialize};

use crate::EffectiveRuntimeLimits;

/// Immutable installed Agent values admitted by one local Host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledAgent {
    /// Exact Agent Definition identity accepted by Session creation.
    pub definition_id: String,
    /// Exact immutable Definition revision.
    pub definition_revision: String,
    /// SHA-256 digest of the effective definition snapshot.
    pub snapshot_digest: String,
    /// Stable namespace used while deriving installed Agent instance identities.
    pub agent_instance_namespace: String,
    /// Effective Runtime limits frozen into each first Execution.
    pub runtime_limits: EffectiveRuntimeLimits,
}

/// Explicit bounds for Host command bodies, event pages, and follow polling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveHostLimits {
    /// Non-zero maximum decoded JSON command bytes.
    pub max_command_bytes: usize,
    /// Non-zero number of durable positions scanned per event page.
    pub event_batch_size: u64,
    /// Non-zero delay between SQLite checks while following events.
    pub event_poll_interval_ms: u64,
}

/// Explicit Runtime clock used to stamp durable Host commands.
pub trait HostClock: Send + Sync {
    /// Returns one RFC 3339 observation time.
    fn recorded_at(&self) -> String;
}

/// Committed start/continuation coordinates delivered after durability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedTurn {
    /// Owning durable Session.
    pub session_id: SessionId,
    /// Durable Turn identity.
    pub turn_id: TurnId,
    /// Fresh disposable Execution identity.
    pub execution_id: ExecutionId,
    /// Session version after the command transaction.
    pub session_version: u64,
    /// Last durable position committed by the command transaction.
    pub committed_position: u64,
}

/// Redacted queue admission failure after a Turn transaction committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnDispatchError;

/// Post-commit boundary that schedules one durable Execution.
pub trait TurnDispatcher: Send + Sync {
    /// Receives committed identities; failure cannot roll back their transaction.
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError>;
}

/// Successful durable Session creation or exact replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateSessionResponse {
    /// Stable durable Session identity.
    pub session_id: String,
    /// Runtime-owned Agent instance bound to the Session.
    pub agent_instance_id: String,
    /// Last position committed by Session creation.
    pub committed_position: u64,
}

/// Successful durable Turn mutation or exact replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnCommandResponse {
    /// Owning Session identity.
    pub session_id: String,
    /// Durable Turn identity.
    pub turn_id: String,
    /// Fresh Execution identity when the command created one.
    pub execution_id: String,
    /// Last position committed by this command.
    pub committed_position: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Exactly one public value representation supplied to a continuation command.
pub enum HostContinuationInput<'a> {
    /// Proto field 4 supplies a UTF-8 string value.
    String(&'a str),
    /// Proto field 5 supplies exact RFC 8785 JSON text.
    Json(&'a str),
}

/// One replayable public event projected from an exact durable fact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LiveHostEvent {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Owning Session identity.
    pub session_id: String,
    /// Source durable fact position; gaps are permitted.
    pub position: u64,
    /// Stable public event name.
    pub event: String,
    /// Owning Turn when the source fact is Turn-scoped.
    pub turn_id: String,
    /// Owning Execution when the source fact supplies one.
    pub execution_id: String,
    /// Redacted display text for a committed completion.
    pub text: String,
}

/// One bounded durable scan used by replay and SSE follow mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostEventPage {
    /// Public events found in the scanned durable range.
    pub events: Vec<LiveHostEvent>,
    /// Highest durable position scanned, including omitted internal facts.
    pub scanned_through_position: u64,
    /// Highest durable position visible when the page was read.
    pub observed_max_position: u64,
}

/// One installed immutable Agent definition visible to product clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentDefinitionSummary {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Stable installed definition identity.
    pub definition_id: String,
    /// Immutable installed definition revision.
    pub definition_revision: String,
    /// Sorted stable public capabilities available to new Sessions.
    pub capabilities: Vec<String>,
}

/// One restart-safe durable Session navigation summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionSummary {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Stable durable Session identity.
    pub session_id: String,
    /// Runtime-owned Agent instance bound to the Session.
    pub agent_instance_id: String,
    /// Immutable definition identity captured when the Session opened.
    pub definition_id: String,
    /// Immutable definition revision captured when the Session opened.
    pub definition_revision: String,
    /// Validated RFC 3339 timestamp of the Session open fact.
    pub opened_at: String,
    /// Frozen last durable position observed for the Session.
    pub latest_position: u64,
    /// Most recently started Turn identity, when present.
    pub latest_turn_id: Option<String>,
    /// Public lifecycle of the most recently started Turn, when present.
    pub latest_turn_state: Option<String>,
    /// Count of verified first-start Turns.
    pub turn_count: u64,
}

/// One complete durable Turn projection for conversation restoration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnTimelineItem {
    /// Stable durable Turn identity.
    pub turn_id: String,
    /// Position of the verified first-start fact.
    pub started_position: u64,
    /// Frozen latest position that changed this Turn.
    pub latest_position: u64,
    /// Stable public lifecycle state.
    pub state: String,
    /// Trusted user text bound to the first start.
    pub user_text: String,
    /// Redacted committed response projection when completed.
    pub completion_text: Option<String>,
    /// Whether bounded display content was truncated.
    pub content_truncated: bool,
}

/// One ascending bounded page of complete durable Turn projections.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnTimelinePage {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Session owning every returned Turn.
    pub session_id: String,
    /// Ascending complete Turn projections.
    pub items: Vec<TurnTimelineItem>,
    /// Durable scan cursor for the next request.
    pub scanned_through_position: u64,
    /// Frozen maximum durable position used for this projection.
    pub observed_max_position: u64,
    /// Whether another bounded timeline page remains.
    pub has_more: bool,
}

/// Stable Host command/query failure with no secret or implementation text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveHostError {
    /// Path, body, idempotency key, installed values, or limits are invalid.
    InvalidRequest,
    /// Requested installed definition, Session, or owned Turn is absent.
    NotFound,
    /// An idempotency identity was reused with different semantics.
    CommandConflict,
    /// Optimistic Session version lost a concurrent mutation race.
    ConcurrentModification,
    /// Current lifecycle or suspension does not admit the command.
    PreconditionFailed,
    /// SQLite could not complete a required durable operation.
    DurabilityUnavailable,
    /// Persisted content failed integrity or exact Host schema validation.
    CorruptState,
}

impl LiveHostError {
    /// Returns the stable machine-readable Host error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::NotFound => "not_found",
            Self::CommandConflict => "command_conflict",
            Self::ConcurrentModification => "concurrent_modification",
            Self::PreconditionFailed => "precondition_failed",
            Self::DurabilityUnavailable => "durability_unavailable",
            Self::CorruptState => "corrupt_state",
        }
    }

    /// Returns a stable operator-safe summary.
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidRequest => "the Host request is invalid",
            Self::NotFound => "the requested Host resource was not found",
            Self::CommandConflict => "the idempotency key conflicts with prior semantics",
            Self::ConcurrentModification => "the durable Session changed concurrently",
            Self::PreconditionFailed => "the durable lifecycle does not admit this command",
            Self::DurabilityUnavailable => "the durable store is unavailable",
            Self::CorruptState => "the durable Host state failed validation",
        }
    }
}

impl fmt::Display for LiveHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl Error for LiveHostError {}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateSessionBody {
    pub agent_definition_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartTurnBody {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CancelTurnBody {
    pub session_id: String,
    pub requested_through_position: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContinueTurnBody {
    pub session_id: String,
    pub suspension_id: String,
    pub expected_session_version: u64,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_json: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ErrorBody {
    pub code: &'static str,
    pub message: &'static str,
}

pub(crate) struct LiveHostState {
    pub database_path: PathBuf,
    pub installed: InstalledAgent,
    pub limits: LiveHostLimits,
    pub clock: Arc<dyn HostClock>,
    pub dispatcher: Arc<dyn TurnDispatcher>,
}
