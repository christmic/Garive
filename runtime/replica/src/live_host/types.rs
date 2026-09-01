use std::{collections::BTreeMap, error::Error, fmt, path::PathBuf, sync::Arc};

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
    /// Sorted stable public capabilities available to newly created Sessions.
    pub public_capabilities: Vec<String>,
    /// Effective Runtime limits frozen into each first Execution.
    pub runtime_limits: EffectiveRuntimeLimits,
    /// Optional snapshot-bound H3 public label catalogue.
    pub public_activity_catalogue: Option<InstalledActivityCatalogue>,
}

/// One snapshot-bound tool identity to public localization-key mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledActivityDescriptor {
    /// Runtime-private provider-neutral tool name.
    pub tool_name: String,
    /// Exact immutable tool revision.
    pub tool_revision: String,
    /// Public stable localization key.
    pub label_key: String,
}

/// Complete immutable H3 catalogue installed with one Agent snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledActivityCatalogue {
    /// Exact catalogue schema version.
    pub schema_version: u32,
    /// Immutable catalogue revision.
    pub catalogue_revision: String,
    /// Canonically sorted unique descriptors.
    pub descriptors: Vec<InstalledActivityDescriptor>,
}

/// Independent bounds for reconstructing and encoding H3 activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityProjectionLimits {
    /// Maximum activities projected for one Turn.
    pub max_activities_per_turn: usize,
    /// Maximum activity-related facts scanned in one fixed prefix.
    pub max_activity_facts: usize,
    /// Maximum UTF-8 bytes in one public localization key.
    pub max_label_bytes: usize,
    /// Maximum UTF-8 bytes in one opaque activity identity.
    pub max_activity_id_bytes: usize,
    /// Maximum canonical JSON bytes across one Turn's activities.
    pub max_encoded_bytes_per_turn: usize,
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
    /// Optional H3 projection bounds; absence keeps H3 unavailable.
    pub activity: Option<ActivityProjectionLimits>,
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
    /// Maximum Goals in one Session projection.
    pub max_goals: usize,
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
    /// Maximum projected Goal objective bytes.
    pub max_goal_objective_bytes: usize,
    /// Maximum decoded or encoded Session cursor bytes.
    pub max_cursor_bytes: usize,
}

impl HostReadLimits {
    /// Product-safe local defaults used by compatibility construction.
    pub const PRODUCT_DEFAULT: Self = Self {
        max_definitions: 64,
        max_sessions: 100,
        max_timeline_items: 100,
        max_goals: 256,
        max_facts: 8_192,
        max_response_bytes: 2 * 1_024 * 1_024,
        max_user_text_bytes: 64 * 1_024,
        max_completion_bytes: 1_024 * 1_024,
        max_prompt_bytes: 64 * 1_024,
        max_goal_objective_bytes: 4 * 1_024,
        max_cursor_bytes: 2_048,
    };

    pub(crate) fn valid(self) -> bool {
        self.max_definitions > 0
            && self.max_sessions > 0
            && self.max_timeline_items > 0
            && self.max_goals > 0
            && self.max_facts > 0
            && self.max_response_bytes > 0
            && self.max_user_text_bytes > 0
            && self.max_completion_bytes > 0
            && self.max_prompt_bytes > 0
            && self.max_goal_objective_bytes > 0
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

/// One bounded, redacted durable Goal projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GoalSummaryV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Stable Goal identity.
    pub goal_id: String,
    /// Current contiguous Goal revision.
    pub revision: u64,
    /// Stable public lifecycle state.
    pub state: &'static str,
    /// Current definition digest without private definition fields.
    pub definition_digest: String,
    /// Bounded objective display text.
    pub objective: String,
    /// Whether objective display text was truncated.
    pub objective_truncated: bool,
    /// Public parent Goal identity, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_goal_id: Option<String>,
    /// Number of distinct attempts started from Draft.
    pub attempt_number: u32,
    /// Number of declared success criteria.
    pub criteria_total: u32,
    /// Number of verified terminal success criteria.
    pub criteria_satisfied: u32,
}

/// Complete bounded Goal page at one Session watermark.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GoalPageV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Owning Session identity.
    pub session_id: String,
    /// Goals in stable identity order.
    pub goals: Vec<GoalSummaryV1>,
    /// Session version used for optimistic commands.
    pub session_version: u64,
    /// Highest durable position included in this response.
    pub observed_max_position: u64,
}

/// Content-free Session transition state consumed only by the mobile Gateway.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct MobileWakeObservation {
    pub session_id: String,
    pub latest_position: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_category: Option<&'static str>,
}

/// Bounded private Runtime snapshot used for Gateway transition detection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct MobileWakePage {
    pub api_version: &'static str,
    pub observations: Vec<MobileWakeObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before: Option<String>,
}

/// Restart-safe public coordinates for one resumable Turn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SuspensionViewV1 {
    /// Exact durable suspension identity.
    pub suspension_id: String,
    /// Session version required by optimistic continuation.
    pub session_version: u64,
    /// Stable suspension family.
    pub kind: String,
    /// Exact public prompt schema identity.
    pub prompt_schema: &'static str,
    /// Canonical RFC 8785 public prompt JSON.
    pub prompt_json: String,
    /// Lowercase SHA-256 of `prompt_json`.
    pub prompt_digest: String,
    /// Canonical portable response schema for interactive suspensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema_json: Option<String>,
    /// Lowercase SHA-256 of `response_schema_json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema_digest: Option<String>,
}

/// One complete Turn projected from a verified fixed Session prefix.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnTimelineItemV1 {
    /// Stable durable Turn identity.
    pub turn_id: String,
    /// Position of the first `turn.started` fact.
    pub started_position: u64,
    /// Latest lifecycle or continuation position for this Turn.
    pub latest_position: u64,
    /// Stable public lifecycle state.
    pub state: String,
    /// Exact verified first trusted-user input.
    pub user_text: String,
    /// Redacted terminal response text when completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_text: Option<String>,
    /// Current restart-safe suspension coordinates when suspended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspension: Option<SuspensionViewV1>,
    /// Whether display text was explicitly bounded by Runtime.
    pub content_truncated: bool,
    /// Latest committed public state for every activity owned by this Turn.
    pub activities: Vec<HostActivity>,
}

/// Bounded ascending page of complete durable Turn projections.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnTimelinePageV1 {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Owning Session identity.
    pub session_id: String,
    /// Complete changed Turns in ascending latest-change order.
    pub items: Vec<TurnTimelineItemV1>,
    /// Highest durable position fully scanned for this page.
    pub scanned_through_position: u64,
    /// Frozen Session watermark used by this response.
    pub observed_max_position: u64,
    /// Whether another bounded scan is required.
    pub has_more: bool,
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
    /// Exact installed Agent Definition identity durably bound to the Session.
    pub definition_id: String,
    /// Exact installed Definition revision durably bound to this Turn.
    pub definition_revision: String,
    /// Exact Effective Agent Snapshot digest durably bound to this Turn.
    pub snapshot_digest: String,
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
    /// Redacted committed H3 state when this event represents activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<HostActivity>,
}

/// One bounded redacted committed Agent activity state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HostActivity {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Opaque Session-scoped activity identity.
    pub activity_id: String,
    /// Stable activity class.
    pub kind: String,
    /// Snapshot-bound public localization key.
    pub label_key: String,
    /// Stable public lifecycle state.
    pub state: String,
    /// Exact committed source position.
    pub source_position: u64,
    /// Authoritative known-state terminal marker.
    pub terminal: bool,
    /// Optional admitted stable safe code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_code: Option<String>,
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

/// Durable path-free Workspace attachment projected for one Session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostWorkspaceAttachment {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Owning durable Session.
    pub session_id: String,
    /// Opaque Workspace capability identity.
    pub workspace_id: String,
    /// Bounded backend-approved display label.
    pub display_name: String,
    /// Exact Workspace grant revision bound at commit.
    pub grant_revision: u64,
    /// Narrow access posture admitted by V1.
    pub access: String,
    /// Durable source position of the attachment.
    pub attached_position: u64,
}

/// Durable path-free receipt for one exact Session Workspace detach command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostWorkspaceDetachment {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Owning durable Session.
    pub session_id: String,
    /// Opaque detached Workspace identity.
    pub workspace_id: String,
    /// Expected grant revision bound by the command.
    pub grant_revision: u64,
    /// Idempotent terminal result.
    pub outcome: String,
    /// Durable source position of the receipt fact.
    pub detached_position: u64,
}

/// Immutable user-visible projection of one committed Artifact revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostArtifact {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Stable Artifact identity.
    pub artifact_id: String,
    /// Immutable revision number.
    pub revision: u64,
    /// Owning Session.
    pub session_id: String,
    /// Owning Turn.
    pub turn_id: String,
    /// Bounded display label.
    pub display_name: String,
    /// Safe coarse kind.
    pub kind: String,
    /// Verified declared MIME type.
    pub mime_type: String,
    /// Exact committed byte count.
    pub byte_size: u64,
    /// SHA-256 digest of committed bytes.
    pub content_digest: String,
    /// Durable Artifact fact position.
    pub committed_position: u64,
    /// Verification posture.
    pub verification: String,
    /// Safe preview posture.
    pub preview: String,
    /// Optional opaque Workspace backing identity.
    pub workspace_id: Option<String>,
    /// Whether an active backing grant may be revealed.
    pub revealable: bool,
    /// Whether an explicit export flow is supported.
    pub exportable: bool,
}

/// One bounded fixed-prefix Artifact page.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HostArtifactPage {
    /// Exact Host API version.
    pub api_version: &'static str,
    /// Owning Session.
    pub session_id: String,
    /// Ascending immutable Artifact revisions.
    pub items: Vec<HostArtifact>,
    /// Highest durable position scanned.
    pub scanned_through_position: u64,
    /// Fixed maximum position observed for this page.
    pub observed_max_position: u64,
    /// Whether another page remains.
    pub has_more: bool,
}

/// Backend-supplied selected Workspace text admitted with one Turn command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostWorkspaceContextEntry {
    /// Opaque entry capability identity.
    pub entry_id: String,
    /// Bounded presentation-only file label.
    pub display_name: String,
    /// Exact coarse content kind; V1 admits text only.
    pub kind: String,
    /// SHA-256 digest of the exact UTF-8 content.
    pub content_digest: String,
    /// Exact bounded UTF-8 content; never a frontend response value.
    pub content_utf8: String,
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
    /// Restart-safe continuation coordinates when the Turn is suspended.
    pub suspension: Option<TurnSuspensionView>,
    /// Whether bounded display content was truncated.
    pub content_truncated: bool,
    /// Latest committed H3 state for each activity owned by this Turn.
    pub activities: Vec<HostActivity>,
}

/// Minimal restart-safe continuation coordinates for Desktop presentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnSuspensionView {
    /// Exact durable suspension identity.
    pub suspension_id: String,
    /// Optimistic Session version required by continuation.
    pub session_version: u64,
    /// Stable public suspension kind.
    pub kind: String,
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
    pub installed: BTreeMap<String, InstalledAgent>,
    pub limits: LiveHostLimits,
    pub read_limits: HostReadLimits,
    pub clock: Arc<dyn HostClock>,
    pub dispatcher: Arc<dyn TurnDispatcher>,
    pub live_output: Option<crate::LiveOutputHub>,
}
