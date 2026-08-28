use garive_llm::{
    ModelCancellation, ModelCapability, ModelItem, ModelOutputSettings, ModelPort,
    ModelStreamEvent, ModelTargetId,
};

use crate::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, ContextRequest, ContextSurface,
    ExecutionId, ExecutionLimits, SessionId, TurnId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResumeInput {
    ExternalInput(String),
    Reconciliation(String),
    ResourceReady,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEntry {
    Start { trusted_input: String },
    Continue { resume_input: ResumeInput },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentCursor {
    pub completed_iterations: u32,
    pub last_durable_position: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissingUsagePolicy {
    Stop,
    Estimate {
        input_tokens: u64,
        output_tokens: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalRecoveryAction {
    Suspend,
    Stop,
    Fail,
    AlternateThenSuspend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputLimitAction {
    CompletePartial,
    Retry { max_retries: u32 },
    Suspend,
    Stop,
    Fail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRecoveryPolicy {
    pub max_context_rebuilds: u32,
    pub output_limit: OutputLimitAction,
    pub transport: TerminalRecoveryAction,
    pub unavailable: TerminalRecoveryAction,
    pub missing_usage: MissingUsagePolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelOnlyLimits {
    pub execution: ExecutionLimits,
    pub max_total_tokens: Option<u64>,
    pub deadline_tick: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTurnRequest {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub execution_id: ExecutionId,
    pub agent_instance_id: AgentInstanceId,
    pub definition_id: AgentDefinitionId,
    pub definition_revision: AgentDefinitionRevision,
    pub entry: AgentEntry,
    pub cursor: AgentCursor,
    pub context_request: ContextRequest,
    pub model_targets: Vec<ModelTargetId>,
    pub required_capabilities: Vec<ModelCapability>,
    pub model_output: ModelOutputSettings,
    pub recovery_policy: ModelRecoveryPolicy,
    pub limits: ModelOnlyLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentRequestError {
    EntryCursorMismatch,
    SessionMismatch,
    MissingModelTarget,
    InvalidModelTarget,
    InvalidTokenLimit,
}

impl AgentTurnRequest {
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
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuspensionReason {
    PartialOutput,
    ResourceUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    IterationLimit,
    TokenLimit,
    Deadline,
    Cancelled,
    ResourceUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentFailureReason {
    InvalidInput,
    InvalidModelOutput,
    RequiredCapabilityUnavailable,
    PortFailure,
    InvariantViolation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentOutcome {
    Completed {
        response_items: Vec<ModelItem>,
        usage: UsageSummary,
    },
    Suspended {
        reason: SuspensionReason,
        partial_items: Vec<ModelItem>,
        last_durable_position: u64,
    },
    Stopped {
        reason: StopReason,
    },
    Failed {
        reason: AgentFailureReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEventKind {
    ExecutionStarted,
    IterationStarted {
        iteration: u32,
    },
    ContextDerived {
        item_count: usize,
        utf8_bytes: usize,
    },
    ModelRequestPrepared {
        request_id: String,
        target_id: String,
    },
    ModelStream(ModelStreamEvent),
    OutcomeProposed,
}

impl AgentEventKind {
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
pub struct AgentEvent {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub execution_id: ExecutionId,
    pub kind: AgentEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortFailure {
    Context,
    Event,
    Clock,
}

pub trait ContextPort {
    fn derive(
        &mut self,
        request: &ContextRequest,
        rebuild_attempt: u32,
    ) -> Result<ContextSurface, PortFailure>;
}

pub trait EventSink {
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure>;
}

pub trait ClockPort {
    fn now_tick(&self) -> Result<u64, PortFailure>;
}

pub struct AgentExecutionPorts<'a> {
    pub context: &'a mut dyn ContextPort,
    pub model: &'a dyn ModelPort,
    pub events: &'a mut dyn EventSink,
    pub cancellation: &'a dyn ModelCancellation,
    pub clock: &'a dyn ClockPort,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionReport {
    pub outcome: AgentOutcome,
    pub completed_iterations: u32,
}
