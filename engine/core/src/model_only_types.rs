use chrono::{DateTime, SecondsFormat, Utc};
use garive_llm::{
    ModelCancellation, ModelCapability, ModelItem, ModelOutputSettings, ModelPort,
    ModelStreamEvent, ModelTargetId, TokenCount,
};
use garive_skill::ActivatedSkill;
use sha2::{Digest, Sha256};

use crate::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, ContextRequest, ContextSurface,
    ExecutionId, ExecutionLimits, SessionId, TurnId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Typed durable input used to begin a fresh execution for an open turn.
pub enum ResumeInput {
    /// Trusted answer to an earlier external-input request.
    ExternalInput(String),
    /// Operator-provided evidence reconciling an uncertain effect.
    Reconciliation(String),
    /// Notification that a previously unavailable dependency can be retried.
    ResourceReady,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// How Runtime enters the turn represented by an execution request.
pub enum AgentEntry {
    /// Starts a new turn with trusted user input and a zero cursor.
    Start {
        /// Input already classified as trusted by Runtime.
        trusted_input: String,
    },
    /// Continues an open turn from durable state with a new execution identity.
    Continue {
        /// Typed evidence satisfying the prior suspension requirement.
        resume_input: ResumeInput,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Durable execution progress reconstructed by Runtime before Core starts.
pub struct AgentCursor {
    /// Iterations already consumed by this turn's bounded policy.
    pub completed_iterations: u32,
    /// Last session-ledger position known to be durably committed.
    pub last_durable_position: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Policy for a provider result that lacks usable token counts.
pub enum MissingUsagePolicy {
    /// Stop rather than treating unknown usage as zero.
    Stop,
    /// Charge a fixed conservative estimate and mark it as estimated.
    Estimate {
        /// Estimated prompt/input tokens per missing record.
        input_tokens: u64,
        /// Estimated generated/output tokens per missing record.
        output_tokens: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Terminal policy action after a recoverable model dependency failure.
pub enum TerminalRecoveryAction {
    /// Preserve a resumable turn and wait for external progress.
    Suspend,
    /// Stop the turn at a policy boundary.
    Stop,
    /// Fail the turn as an execution error.
    Fail,
    /// Try remaining frozen targets, then suspend if none succeeds.
    AlternateThenSuspend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Policy action for a model response interrupted at its output limit.
pub enum OutputLimitAction {
    /// Accept valid partial non-tool output as the completed response.
    CompletePartial,
    /// Issue at most the declared number of fresh bounded attempts.
    Retry {
        /// Maximum additional model requests after output-limit interruption.
        max_retries: u32,
    },
    /// Preserve partial output and wait for an external continuation decision.
    Suspend,
    /// Stop the turn at a policy boundary.
    Stop,
    /// Fail because complete output is required.
    Fail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Immutable and bounded recovery decisions for model-only execution.
pub struct ModelRecoveryPolicy {
    /// Maximum context rebuilds after context-overflow rejection.
    pub max_context_rebuilds: u32,
    /// Action applied to output-limit interruption.
    pub output_limit: OutputLimitAction,
    /// Action applied to transport interruption.
    pub transport: TerminalRecoveryAction,
    /// Action applied when a selected model target is unavailable.
    pub unavailable: TerminalRecoveryAction,
    /// Accounting action when provider usage is missing.
    pub missing_usage: MissingUsagePolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Execution, token, and logical-deadline bounds for model-only Core.
pub struct ModelOnlyLimits {
    /// Non-zero iteration limit enforced by [`crate::ExecutionControl`].
    pub execution: ExecutionLimits,
    /// Optional accumulated input-plus-output token limit.
    pub max_total_tokens: Option<u64>,
    /// Optional inclusive Runtime clock tick at which work must stop.
    pub deadline_tick: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Exact durable evidence attribution attached to model-visible Memory data.
pub struct MemoryEvidenceAttribution {
    /// Session containing the supporting fact.
    pub session_id: String,
    /// Exact non-zero Session-local fact position.
    pub position: u64,
    /// Durable fact identity.
    pub fact_id: String,
    /// Canonical payload digest of the supporting fact.
    pub payload_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Runtime-verified inline Memory content supplied as subordinate attributed data.
pub struct AttributedMemory {
    /// Stable logical record identity.
    pub record_id: String,
    /// Exact immutable revision identity.
    pub revision_id: String,
    /// SHA-256 digest binding `content_utf8`.
    pub content_digest: String,
    /// Resolved inline UTF-8 content; references are resolved by Runtime first.
    pub content_utf8: String,
    /// Ordered non-empty durable provenance.
    pub evidence: Vec<MemoryEvidenceAttribution>,
}

impl AttributedMemory {
    pub(crate) fn valid(&self) -> bool {
        !self.record_id.is_empty()
            && !self.revision_id.is_empty()
            && !self.content_utf8.is_empty()
            && digest(self.content_utf8.as_bytes()) == self.content_digest
            && !self.evidence.is_empty()
            && self.evidence.iter().all(MemoryEvidenceAttribution::valid)
    }
}

impl MemoryEvidenceAttribution {
    fn valid(&self) -> bool {
        !self.session_id.is_empty()
            && self.position != 0
            && !self.fact_id.is_empty()
            && valid_digest(&self.payload_digest)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Exact sanitized citation attached to model-visible Knowledge evidence.
pub struct KnowledgeCitationAttribution {
    /// Portable citation locator kind.
    pub locator_kind: String,
    /// Sanitized connector locator.
    pub locator: String,
    /// Optional display title.
    pub title: Option<String>,
    /// Optional canonical public URI.
    pub canonical_uri: Option<String>,
    /// Digest binding the cited content.
    pub content_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Runtime-verified Knowledge evidence supplied as subordinate attributed data.
pub struct AttributedKnowledge {
    /// Exact configured source identity.
    pub source_id: String,
    /// Exact configured source revision.
    pub source_revision: String,
    /// Stable evidence identity within the source response.
    pub evidence_id: String,
    /// Optional exact source snapshot.
    pub source_snapshot_digest: Option<String>,
    /// SHA-256 digest binding `content_utf8`.
    pub content_digest: String,
    /// Runtime-resolved inline UTF-8 evidence.
    pub content_utf8: String,
    /// Runtime-verified UTF-8 byte length.
    pub content_byte_length: u64,
    /// Exact sanitized citation.
    pub citation: KnowledgeCitationAttribution,
    /// Canonical UTC retrieval timestamp.
    pub retrieved_at_utc: String,
    /// `fresh`, `cached`, or `stale`.
    pub freshness: String,
    /// `curated`, `first_party`, `third_party`, or `untrusted`.
    pub trust_class: String,
    /// Connector rank metadata in basis points.
    pub rank_basis_points: u16,
}

impl AttributedKnowledge {
    pub(crate) fn valid(&self) -> bool {
        !self.source_id.is_empty()
            && !self.source_revision.is_empty()
            && !self.evidence_id.is_empty()
            && self
                .source_snapshot_digest
                .as_deref()
                .map_or(true, valid_digest)
            && !self.content_utf8.is_empty()
            && digest(self.content_utf8.as_bytes()) == self.content_digest
            && self.content_byte_length == self.content_utf8.len() as u64
            && self.citation.valid(&self.content_digest)
            && canonical_utc(&self.retrieved_at_utc)
            && matches!(self.freshness.as_str(), "fresh" | "cached" | "stale")
            && matches!(
                self.trust_class.as_str(),
                "curated" | "first_party" | "third_party" | "untrusted"
            )
            && self.rank_basis_points <= 10_000
    }
}

impl KnowledgeCitationAttribution {
    fn valid(&self, content_digest: &str) -> bool {
        matches!(
            self.locator_kind.as_str(),
            "uri_fragment" | "document_offset" | "record_key" | "opaque_locator"
        ) && !self.locator.is_empty()
            && self.title.as_ref().map_or(true, |value| !value.is_empty())
            && self
                .canonical_uri
                .as_ref()
                .map_or(true, |value| !value.is_empty())
            && self.content_digest == content_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Complete immutable input for one disposable model-only kernel execution.
pub struct AgentTurnRequest {
    /// Session owning the turn and its ledger facts.
    pub session_id: SessionId,
    /// Durable turn identity shared across continuations.
    pub turn_id: TurnId,
    /// Unique identity for this disposable execution attempt.
    pub execution_id: ExecutionId,
    /// Runtime-owned installed agent instance.
    pub agent_instance_id: AgentInstanceId,
    /// Agent definition selected for the instance.
    pub definition_id: AgentDefinitionId,
    /// Frozen agent definition revision.
    pub definition_revision: AgentDefinitionRevision,
    /// Start or typed continuation input.
    pub entry: AgentEntry,
    /// Durable progress reconstructed before invocation.
    pub cursor: AgentCursor,
    /// Frozen context window and budgets.
    pub context_request: ContextRequest,
    /// Exact Skills durably activated before any model request.
    pub activated_skills: Vec<ActivatedSkill>,
    /// Ordered Runtime-verified optional Memory data committed before model use.
    pub attributed_memory: Vec<AttributedMemory>,
    /// Ordered Runtime-verified Knowledge evidence committed before model use.
    pub attributed_knowledge: Vec<AttributedKnowledge>,
    /// Ordered frozen model targets available to recovery policy.
    pub model_targets: Vec<ModelTargetId>,
    /// Provider-neutral capabilities every selected target must satisfy.
    pub required_capabilities: Vec<ModelCapability>,
    /// Provider-neutral output constraints.
    pub model_output: ModelOutputSettings,
    /// Bounded model recovery policy.
    pub recovery_policy: ModelRecoveryPolicy,
    /// Execution, usage, and deadline limits.
    pub limits: ModelOnlyLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Validation failure in an immutable agent execution request.
pub enum AgentRequestError {
    /// Start/continue mode conflicts with its reconstructed durable cursor.
    EntryCursorMismatch,
    /// Context was frozen for a different session.
    SessionMismatch,
    /// No model target was supplied.
    MissingModelTarget,
    /// A supplied model target has an empty opaque identity.
    InvalidModelTarget,
    /// An optional total-token limit was explicitly set to zero.
    InvalidTokenLimit,
    /// A model-visible Memory value lacks an exact content/evidence binding.
    InvalidMemoryContext,
    /// A model-visible Knowledge value lacks an exact content/citation binding.
    InvalidKnowledgeContext,
}

impl AgentTurnRequest {
    /// Validates cross-field request invariants before any port is invoked.
    pub fn validate(&self) -> Result<(), AgentRequestError> {
        match &self.entry {
            AgentEntry::Start { .. }
                if self.cursor.completed_iterations != 0
                    || self.cursor.last_durable_position != 0 =>
            {
                return Err(AgentRequestError::EntryCursorMismatch);
            }
            AgentEntry::Continue { .. } if self.cursor.last_durable_position == 0 => {
                return Err(AgentRequestError::EntryCursorMismatch);
            }
            _ => {}
        }
        if self.context_request.session_id != self.session_id.as_str() {
            return Err(AgentRequestError::SessionMismatch);
        }
        if self.model_targets.is_empty() {
            return Err(AgentRequestError::MissingModelTarget);
        }
        if self
            .model_targets
            .iter()
            .any(|target| target.as_str().is_empty())
        {
            return Err(AgentRequestError::InvalidModelTarget);
        }
        if self.limits.max_total_tokens == Some(0) {
            return Err(AgentRequestError::InvalidTokenLimit);
        }
        if self.attributed_memory.iter().any(|value| !value.valid()) {
            return Err(AgentRequestError::InvalidMemoryContext);
        }
        if self.attributed_knowledge.iter().any(|value| !value.valid()) {
            return Err(AgentRequestError::InvalidKnowledgeContext);
        }
        Ok(())
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn canonical_utc(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Checked token usage accumulated across all attempts in one execution.
pub struct UsageSummary {
    /// Total accounted input tokens, or unknown provider evidence.
    pub input_tokens: TokenCount,
    /// Total accounted output tokens, or unknown provider evidence.
    pub output_tokens: TokenCount,
    /// Whether any component came from the configured conservative estimate.
    pub estimated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Typed reason a turn remains open after the current execution closes.
pub enum SuspensionReason {
    /// A governed effect requires a durable approval response.
    ApprovalRequired,
    /// A governed effect requires typed external input.
    ExternalInputRequired,
    /// An uncertain effect requires operator evidence.
    OperatorReconciliation,
    /// Valid partial model output requires later continuation.
    PartialOutput,
    /// No frozen model resource can currently serve the request.
    ResourceUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Policy boundary that closes a turn without classifying it as failure.
pub enum StopReason {
    /// Starting another model attempt would exceed the iteration bound.
    IterationLimit,
    /// Known or conservatively estimated usage reaches a token bound.
    TokenLimit,
    /// Runtime's logical clock reaches the frozen deadline.
    Deadline,
    /// Cooperative cancellation was observed at a defined boundary.
    Cancelled,
    /// Policy chose to stop after model unavailability.
    ResourceUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Non-success reason classified as an execution failure.
pub enum AgentFailureReason {
    /// Request validation failed before execution.
    InvalidInput,
    /// Model output violated the provider-neutral contract.
    InvalidModelOutput,
    /// Valid model output requires a capability absent from this execution.
    RequiredCapabilityUnavailable,
    /// A frozen execution port failed.
    PortFailure,
    /// Core detected an impossible internal transition or arithmetic state.
    InvariantViolation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Exactly one terminal proposal produced by a kernel execution.
pub enum AgentOutcome {
    /// Intended response completed successfully.
    Completed {
        /// Ordered provider-neutral response items.
        response_items: Vec<ModelItem>,
        /// Usage accumulated across all model attempts.
        usage: UsageSummary,
    },
    /// Current execution closed while the durable turn remains resumable.
    Suspended {
        /// Typed requirement for a future continuation.
        reason: SuspensionReason,
        /// Valid model output preserved for Runtime to commit.
        partial_items: Vec<ModelItem>,
        /// Durable ledger cursor from the immutable request.
        last_durable_position: u64,
        /// Exact Runtime-owned binding for governed suspensions only.
        governed_binding: Option<crate::GovernedSuspensionBinding>,
    },
    /// Work ended at an expected policy boundary.
    Stopped {
        /// Boundary that caused the stop.
        reason: StopReason,
    },
    /// Execution could not produce a valid policy-compliant result.
    Failed {
        /// Stable failure classification without provider secrets.
        reason: AgentFailureReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Ordered semantic progress emitted by Core for Runtime to persist or publish.
pub enum AgentEventKind {
    /// The immutable request passed initial execution setup.
    ExecutionStarted,
    /// The controller entered one bounded iteration.
    IterationStarted {
        /// One-based iteration number.
        iteration: u32,
    },
    /// Context was derived without exposing its content in the event.
    ContextDerived {
        /// Number of derived input or redaction items.
        item_count: usize,
        /// Charged visible UTF-8 bytes.
        utf8_bytes: usize,
    },
    /// A provider-neutral request was frozen before invoking the model port.
    ModelRequestPrepared {
        /// Unique logical request identity.
        request_id: String,
        /// Frozen selected model target identity.
        target_id: String,
    },
    /// Normalized streaming progress from the model adapter.
    ModelStream(ModelStreamEvent),
    /// Exactly one terminal outcome is about to be returned to Runtime.
    OutcomeProposed,
}

impl AgentEventKind {
    /// Returns the stable machine-readable event kind.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ExecutionStarted => "execution-started",
            Self::IterationStarted { .. } => "iteration-started",
            Self::ContextDerived { .. } => "context-derived",
            Self::ModelRequestPrepared { .. } => "model-request-prepared",
            Self::ModelStream(_) => "model-stream",
            Self::OutcomeProposed => "outcome-proposed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Semantic event with the identities needed for ordered attribution.
pub struct AgentEvent {
    /// Owning session.
    pub session_id: SessionId,
    /// Owning durable turn.
    pub turn_id: TurnId,
    /// Disposable execution that emitted the event.
    pub execution_id: ExecutionId,
    /// Provider-neutral semantic event payload.
    pub kind: AgentEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Sanitized class of a frozen execution-port failure.
pub enum PortFailure {
    /// Context derivation dependency failed.
    Context,
    /// Event delivery dependency failed.
    Event,
    /// Logical clock dependency failed.
    Clock,
    /// Durable governed-effect dependency failed.
    Tool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Failure returned specifically by [`ContextPort`].
pub enum ContextPortError {
    /// Required durable facts cannot fit in the frozen context budget.
    RequiredFactsExceedBudget,
    /// Context storage, projection, or adapter failed.
    PortFailure,
}

/// Frozen port that derives purpose-specific context from durable facts.
pub trait ContextPort {
    /// Derives the requested surface for one bounded rebuild attempt.
    fn derive(
        &mut self,
        request: &ContextRequest,
        rebuild_attempt: u32,
    ) -> Result<ContextSurface, ContextPortError>;
}

/// Sink for ordered semantic progress; emission is not proof of persistence.
pub trait EventSink: Send {
    /// Emits one event or returns a sanitized port failure.
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure>;
}

/// Runtime-owned monotonic logical clock used for deterministic deadlines.
pub trait ClockPort {
    /// Returns the current logical tick without reading wall time inside Core.
    fn now_tick(&self) -> Result<u64, PortFailure>;
}

/// Frozen external capabilities available to one model-only execution.
pub struct AgentExecutionPorts<'a> {
    /// Purpose-specific context derivation capability.
    pub context: &'a mut dyn ContextPort,
    /// Provider-neutral model invocation capability.
    pub model: &'a dyn ModelPort,
    /// Ordered semantic event sink.
    pub events: &'a mut dyn EventSink,
    /// Cooperative cancellation signal shared with the model adapter.
    pub cancellation: &'a dyn ModelCancellation,
    /// Runtime logical clock for deadline checks.
    pub clock: &'a dyn ClockPort,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Complete result returned to Runtime after the control object closes.
pub struct ExecutionReport {
    /// Exactly one terminal outcome proposal.
    pub outcome: AgentOutcome,
    /// Durable iteration cursor Runtime should commit with the outcome.
    pub completed_iterations: u32,
    /// Cumulative token evidence for every terminal path.
    pub usage: UsageSummary,
}
