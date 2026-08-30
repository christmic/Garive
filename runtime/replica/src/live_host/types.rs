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

/// Independent bounds for client-safe Host read projections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostReadLimits {
    /// Maximum installed definitions in one response.
    pub max_definitions: usize,
    /// Maximum Sessions in one page.
    pub max_sessions: usize,
    /// Maximum complete Turns in one timeline page.
    pub max_timeline_items: usize,
    /// Maximum durable facts scanned for one projection.
    pub max_facts: usize,
    /// Maximum encoded JSON response bytes.
    pub max_response_bytes: usize,
    /// Maximum projected user input bytes per Turn.
    pub max_user_text_bytes: usize,
    /// Maximum projected completion bytes per Turn.
    pub max_completion_bytes: usize,
    /// Maximum public suspension prompt or schema bytes.
    pub max_prompt_bytes: usize,
    /// Maximum decoded or encoded Session cursor bytes.
    pub max_cursor_bytes: usize,
}

impl HostReadLimits {
    /// Product-safe local defaults used by compatibility construction.
    pub const PRODUCT_DEFAULT: Self = Self {
        max_definitions: 64,
        max_sessions: 100,
        max_timeline_items: 100,
        max_facts: 8_192,
        max_response_bytes: 2 * 1_024 * 1_024,
        max_user_text_bytes: 64 * 1_024,
        max_completion_bytes: 1_024 * 1_024,
        max_prompt_bytes: 64 * 1_024,
        max_cursor_bytes: 2_048,
    };

    pub(crate) fn valid(self) -> bool {
        self.max_definitions > 0
            && self.max_sessions > 0
            && self.max_timeline_items > 0
            && self.max_facts > 0
            && self.max_response_bytes > 0
            && self.max_user_text_bytes > 0
            && self.max_completion_bytes > 0
            && self.max_prompt_bytes > 0
            && self.max_cursor_bytes > 0
    }
}

/// One installed Agent definition safe for client discovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentDefinitionSummaryV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Immutable Agent definition identity.
    pub definition_id: String,
    /// Immutable Agent definition revision.
    pub definition_revision: String,
    /// Sorted stable public capability names.
    pub capabilities: Vec<String>,
}

/// Bounded installed Agent definition result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentDefinitionPageV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Installed definitions in stable identity order.
    pub definitions: Vec<AgentDefinitionSummaryV1>,
}

/// Restart-safe summary of one verified durable Session prefix.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionSummaryV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Stable durable Session identity.
    pub session_id: String,
    /// Runtime-owned Agent instance identity.
    pub agent_instance_id: String,
    /// Immutable installed definition identity.
    pub definition_id: String,
    /// Immutable installed definition revision.
    pub definition_revision: String,
    /// RFC 3339 time from the verified opening fact.
    pub opened_at: String,
    /// Frozen highest durable Session position.
    pub latest_position: u64,
    /// Most recently first-started Turn, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_turn_id: Option<String>,
    /// Stable lifecycle of the latest Turn, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_turn_state: Option<String>,
    /// Count of verified first-start facts.
    pub turn_count: u64,
}

/// One exact Session summary at a frozen durable watermark.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionViewV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Verified Session summary.
    pub session: SessionSummaryV1,
    /// Highest durable position included in this response.
    pub observed_max_position: u64,
}

/// Reverse-opened bounded page of verified durable Sessions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionPageV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Session summaries in the requested page.
    pub sessions: Vec<SessionSummaryV1>,
    /// Opaque cursor for the next older page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before: Option<String>,
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
    /// A verified read projection cannot fit configured public bounds.
    ReadBoundExceeded,
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
            Self::ReadBoundExceeded => "read_bound_exceeded",
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
            Self::ReadBoundExceeded => "the Host read result exceeds configured bounds",
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
    pub input: Option<String>,
    pub input_json: Option<serde_json::Value>,
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
    pub read_limits: HostReadLimits,
    pub clock: Arc<dyn HostClock>,
    pub dispatcher: Arc<dyn TurnDispatcher>,
}
