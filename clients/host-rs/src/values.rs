//! Public values shared by the Rust Host clients.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

/// Explicit transport and reduction bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientLimits {
    /// Maximum serialized mutation body size.
    pub max_command_bytes: usize,
    /// Maximum H1 JSON event or bounded response size; H4 has fixed v1 bounds.
    pub max_event_bytes: usize,
    /// Maximum events accepted by one follow operation.
    pub max_events: usize,
    /// Whole follow-operation deadline in milliseconds.
    pub follow_deadline_ms: u64,
}

/// Exact H1 public event consumed by Rust presentation surfaces.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostEvent {
    /// Host API version; v1 clients accept only `v1`.
    pub api_version: String,
    /// Owning Session identity.
    pub session_id: String,
    /// Durable Session position.
    pub position: u64,
    /// Stable or future event name.
    pub event: String,
    /// Turn identity, empty when not applicable.
    pub turn_id: String,
    /// Execution identity, empty when not applicable.
    pub execution_id: String,
    /// Committed presentation text, empty when not applicable.
    pub text: String,
    /// Public Agent activity state when this is an H3 event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<HostActivity>,
}

/// One strictly validated ephemeral H4 event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveOutputEvent {
    /// Exact H4 API version.
    pub api_version: String,
    /// Owning Session identity.
    pub session_id: String,
    /// Owning Turn identity.
    pub turn_id: String,
    /// Owning Execution identity.
    pub execution_id: String,
    /// Ephemeral publisher generation UUID.
    pub stream_id: String,
    /// Monotonic sequence within this generation.
    pub sequence: u64,
    /// Closed public H4 payload.
    pub kind: LiveOutputEventKind,
}

/// Closed public payload variants for progressive output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LiveOutputEventKind {
    /// Full in-memory preview at subscription time.
    Snapshot {
        /// Exact complete preview text.
        text: String,
        /// Latest sequence represented by the snapshot.
        through_sequence: u64,
    },
    /// Ordered answer suffix.
    TextDelta {
        /// Exact non-empty UTF-8 suffix.
        text: String,
    },
    /// Closed public work phase.
    PhaseChanged {
        /// Stable public phase code.
        phase: String,
        /// Matching stable localization key.
        label_key: String,
    },
    /// Complete preview is unavailable and must be cleared.
    PreviewUnavailable,
    /// Ephemeral publisher ended without granting terminal authority.
    Ended {
        /// Safe end classification.
        reason: LiveOutputEndReason,
    },
}

/// Stable safe reason carried by an H4 end marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveOutputEndReason {
    /// A completed durable terminal was committed.
    TerminalCommitted,
    /// A durable suspension was committed.
    Suspended,
    /// A durable stop was committed.
    Stopped,
    /// A durable failure was committed.
    Failed,
    /// The publisher closed without claiming a durable terminal.
    PublisherClosed,
}

/// One redacted committed Agent interaction or tool activity state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostActivity {
    /// Exact Host API version.
    pub api_version: String,
    /// Opaque activity identity scoped to the Session.
    pub activity_id: String,
    /// Stable or unknown future activity family.
    pub kind: String,
    /// Admitted localization key.
    pub label_key: String,
    /// Stable or unknown future state.
    pub state: String,
    /// Durable source position.
    pub source_position: u64,
    /// Authoritative terminal marker.
    pub terminal: bool,
    /// Optional admitted stable code.
    pub safe_code: Option<String>,
}

/// Durable terminal state recognized by A1 clients.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostTerminal {
    /// Turn committed successful completion.
    Completed,
    /// Turn committed a resumable suspension.
    Suspended,
    /// Turn committed a stop.
    Stopped,
    /// Turn committed failure.
    Failed,
}

/// Ephemeral projection reconstructed from durable Host events.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HostView {
    /// Highest applied durable position.
    pub cursor: u64,
    /// Recognized terminal state, if committed.
    pub terminal: Option<HostTerminal>,
    /// Committed completion text.
    pub text: String,
    /// Unknown event names retained for forward-compatible diagnostics.
    pub unknown_events: Vec<String>,
    /// Latest committed state of each observed public activity.
    pub activities: BTreeMap<String, HostActivity>,
    /// Applied event fingerprints used to verify duplicate positions.
    pub(crate) seen: BTreeMap<u64, HostEvent>,
}

impl HostView {
    /// Creates an empty reconnect projection at a previously saved cursor.
    pub fn at_cursor(cursor: u64) -> Self {
        Self {
            cursor,
            ..Self::default()
        }
    }
}

/// H1 create-Session response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CreateSessionResponse {
    /// New Session identity.
    pub session_id: String,
    /// Instantiated Agent identity.
    pub agent_instance_id: String,
    /// Durable commit position.
    pub committed_position: u64,
}

/// H1 start-Turn response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TurnCommandResponse {
    /// Owning Session identity.
    pub session_id: String,
    /// New Turn identity.
    pub turn_id: String,
    /// Initial Execution identity.
    pub execution_id: String,
    /// Durable commit position.
    pub committed_position: u64,
}

/// One installed immutable Agent definition safe for discovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AgentDefinitionSummary {
    /// Exact Host API version.
    pub api_version: String,
    /// Immutable definition identity.
    pub definition_id: String,
    /// Immutable definition revision.
    pub definition_revision: String,
    /// Sorted stable public capability names.
    pub capabilities: Vec<String>,
}

/// Bounded installed Agent definition result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AgentDefinitionPage {
    /// Exact Host API version.
    pub api_version: String,
    /// Installed definitions in stable identity order.
    pub definitions: Vec<AgentDefinitionSummary>,
}

/// Restart-safe summary of one durable Session prefix.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SessionSummary {
    /// Exact Host API version.
    pub api_version: String,
    /// Stable Session identity.
    pub session_id: String,
    /// Runtime-owned Agent instance identity.
    pub agent_instance_id: String,
    /// Immutable definition identity.
    pub definition_id: String,
    /// Immutable definition revision.
    pub definition_revision: String,
    /// RFC 3339 opening time.
    pub opened_at: String,
    /// Frozen highest durable Session position.
    pub latest_position: u64,
    /// Most recently first-started Turn.
    pub latest_turn_id: Option<String>,
    /// Stable lifecycle of the latest Turn.
    pub latest_turn_state: Option<String>,
    /// Number of verified first-started Turns.
    pub turn_count: u64,
}

/// Reverse-opened bounded page of durable Sessions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SessionPage {
    /// Exact Host API version.
    pub api_version: String,
    /// Session summaries in page order.
    pub sessions: Vec<SessionSummary>,
    /// Opaque cursor for the next older page.
    pub next_before: Option<String>,
}

/// One exact Session summary at a frozen watermark.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SessionView {
    /// Exact Host API version.
    pub api_version: String,
    /// Verified Session summary.
    pub session: SessionSummary,
    /// Highest durable position included in this response.
    pub observed_max_position: u64,
}

/// One bounded, redacted durable Goal projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct GoalSummary {
    /// Exact Host API version.
    pub api_version: String,
    /// Stable Goal identity.
    pub goal_id: String,
    /// Current contiguous Goal revision.
    pub revision: u64,
    /// Stable public lifecycle state.
    pub state: String,
    /// Lowercase SHA-256 of the current private definition.
    pub definition_digest: String,
    /// Bounded objective display text.
    pub objective: String,
    /// Whether objective display text was truncated.
    pub objective_truncated: bool,
    /// Parent Goal identity, when present.
    pub parent_goal_id: Option<String>,
    /// Number of attempts started from Draft.
    pub attempt_number: u32,
    /// Number of declared success criteria.
    pub criteria_total: u32,
    /// Number of verified terminal success criteria.
    pub criteria_satisfied: u32,
}

/// Complete bounded Goal page at one Session watermark.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct GoalPage {
    /// Exact Host API version.
    pub api_version: String,
    /// Owning Session identity.
    pub session_id: String,
    /// Goals in stable identity order.
    pub goals: Vec<GoalSummary>,
    /// Session version used for optimistic commands.
    pub session_version: u64,
    /// Highest durable position included in this response.
    pub observed_max_position: u64,
}

/// One bounded redacted durable Plan revision projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PlanSummary {
    /// Exact Host API version.
    pub api_version: String,
    /// Stable Plan identity.
    pub plan_id: String,
    /// Immutable Plan revision.
    pub revision: u64,
    /// Stable public lifecycle state.
    pub state: String,
    /// Lowercase SHA-256 of the private definition.
    pub definition_digest: String,
    /// Bound Goal identity.
    pub goal_id: String,
    /// Bound Goal revision.
    pub goal_revision: u64,
    /// Contiguous mutable Plan state version.
    pub state_version: u64,
    /// Total declared steps.
    pub steps_total: u32,
    /// Steps currently ready to claim.
    pub steps_ready: u32,
    /// Claimed, running or suspended steps.
    pub steps_active: u32,
    /// Verified completed steps.
    pub steps_completed: u32,
    /// Steps whose latest attempt failed.
    pub steps_failed: u32,
    /// Total attempts started across the revision.
    pub total_attempts: u32,
}

/// Complete bounded Plan revision page at one Session watermark.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PlanPage {
    /// Exact Host API version.
    pub api_version: String,
    /// Owning Session identity.
    pub session_id: String,
    /// Plan revisions in stable identity then revision order.
    pub plans: Vec<PlanSummary>,
    /// Session version used for optimistic commands.
    pub session_version: u64,
    /// Highest durable position included in this response.
    pub observed_max_position: u64,
}

/// Restart-safe coordinates and public schemas for one suspension.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SuspensionView {
    /// Exact durable suspension identity.
    pub suspension_id: String,
    /// Session version required by continuation.
    pub session_version: u64,
    /// Stable suspension family.
    pub kind: String,
    /// Exact public prompt schema identity.
    pub prompt_schema: String,
    /// Canonical public prompt JSON.
    pub prompt_json: String,
    /// Lowercase SHA-256 of the prompt.
    pub prompt_digest: String,
    /// Canonical portable response schema when interactive.
    pub response_schema_json: Option<String>,
    /// Lowercase SHA-256 of the response schema.
    pub response_schema_digest: Option<String>,
}

/// One complete Turn in a durable conversation timeline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TurnTimelineItem {
    /// Stable Turn identity.
    pub turn_id: String,
    /// Position of the first Turn start.
    pub started_position: u64,
    /// Latest lifecycle or continuation position.
    pub latest_position: u64,
    /// Stable public lifecycle state.
    pub state: String,
    /// Verified first trusted-user input.
    pub user_text: String,
    /// Redacted committed completion text.
    pub completion_text: Option<String>,
    /// Current suspension coordinates.
    pub suspension: Option<SuspensionView>,
    /// Whether Runtime explicitly truncated display content.
    pub content_truncated: bool,
    /// Latest public state of each committed Agent activity.
    #[serde(default)]
    pub activities: Vec<HostActivity>,
}

/// Bounded page of complete durable Turn projections.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TurnTimelinePage {
    /// Exact Host API version.
    pub api_version: String,
    /// Owning Session identity.
    pub session_id: String,
    /// Complete changed Turns in latest-change order.
    pub items: Vec<TurnTimelineItem>,
    /// Highest durable position fully scanned.
    pub scanned_through_position: u64,
    /// Frozen Session watermark used by this response.
    pub observed_max_position: u64,
    /// Whether another bounded scan is required.
    pub has_more: bool,
}

/// Stable safe client failure categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostClientErrorCode {
    /// Constructor input is invalid.
    InvalidConfiguration,
    /// Mutation or follow command is invalid.
    InvalidCommand,
    /// Response or event violates the H1 schema.
    InvalidEvent,
    /// Event ordering or duplicate identity is inconsistent.
    EventOrderViolation,
    /// A configured event bound was exceeded.
    EventLimitExceeded,
    /// Host returned a known stable H1 failure.
    HostFailure,
    /// Host returned an unknown future failure.
    UnknownHostError,
    /// The mobile access grant is absent, expired, invalid, or revoked.
    AuthenticationRequired,
    /// The authenticated mobile actor lacks authority for the resource.
    ActorForbidden,
    /// The device binding must be established again.
    DeviceReauthRequired,
    /// Gateway admission refused the request before routing it.
    RateLimited,
    /// The bound Runtime route is unavailable.
    RuntimeUnavailable,
    /// The one-time pairing ceremony was rejected.
    PairingRejected,
    /// HTTP or stream transport failed.
    TransportFailure,
    /// The configured follow deadline elapsed.
    FollowDeadline,
}

impl HostClientErrorCode {
    /// Returns the portable lower-snake-case failure name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::InvalidCommand => "invalid_command",
            Self::InvalidEvent => "invalid_event",
            Self::EventOrderViolation => "event_order_violation",
            Self::EventLimitExceeded => "event_limit_exceeded",
            Self::HostFailure => "host_failure",
            Self::UnknownHostError => "unknown_host_error",
            Self::AuthenticationRequired => "authentication_required",
            Self::ActorForbidden => "actor_forbidden",
            Self::DeviceReauthRequired => "device_reauth_required",
            Self::RateLimited => "rate_limited",
            Self::RuntimeUnavailable => "runtime_unavailable",
            Self::PairingRejected => "pairing_rejected",
            Self::TransportFailure => "transport_failure",
            Self::FollowDeadline => "follow_deadline",
        }
    }
}

/// Complete ordered A1 failure vocabulary.
pub const HOST_CLIENT_FAILURES: [HostClientErrorCode; 15] = [
    HostClientErrorCode::InvalidConfiguration,
    HostClientErrorCode::InvalidCommand,
    HostClientErrorCode::InvalidEvent,
    HostClientErrorCode::EventOrderViolation,
    HostClientErrorCode::EventLimitExceeded,
    HostClientErrorCode::HostFailure,
    HostClientErrorCode::UnknownHostError,
    HostClientErrorCode::AuthenticationRequired,
    HostClientErrorCode::ActorForbidden,
    HostClientErrorCode::DeviceReauthRequired,
    HostClientErrorCode::RateLimited,
    HostClientErrorCode::RuntimeUnavailable,
    HostClientErrorCode::PairingRejected,
    HostClientErrorCode::TransportFailure,
    HostClientErrorCode::FollowDeadline,
];

/// Safe client error that never retains request, header, event, or body content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostClientError {
    /// Portable failure category.
    pub code: HostClientErrorCode,
    /// HTTP status when a Host error response was received.
    pub status: Option<u16>,
}

impl HostClientError {
    pub(crate) const fn new(code: HostClientErrorCode) -> Self {
        Self { code, status: None }
    }

    pub(crate) const fn with_status(code: HostClientErrorCode, status: u16) -> Self {
        Self {
            code,
            status: Some(status),
        }
    }
}

impl fmt::Display for HostClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(status) => write!(formatter, "{} (HTTP {status})", self.code.wire_name()),
            None => formatter.write_str(self.code.wire_name()),
        }
    }
}

impl std::error::Error for HostClientError {}
