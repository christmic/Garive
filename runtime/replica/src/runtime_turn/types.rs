use std::{error::Error, fmt};

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, ExecutionId, FactDraft, SessionId,
    TurnId,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
/// Non-empty caller-supplied identity binding one idempotent Runtime command.
pub struct RuntimeCommandId(Box<str>);

impl RuntimeCommandId {
    /// Validates and constructs a command identity.
    pub fn new(value: impl Into<Box<str>>) -> Result<Self, RuntimeCommandError> {
        let value = value.into();
        if value.is_empty() {
            Err(RuntimeCommandError::InvalidCommand)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the command identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Effective limits frozen into one durable Execution start.
pub struct EffectiveRuntimeLimits {
    /// Non-zero maximum completed iterations.
    pub max_iterations: u64,
    /// Optional non-zero input-token limit.
    pub max_input_tokens: Option<u64>,
    /// Optional non-zero output-token limit.
    pub max_output_tokens: Option<u64>,
    /// Optional non-zero deadline budget in milliseconds.
    pub deadline_budget_ms: Option<u64>,
}

impl EffectiveRuntimeLimits {
    pub(crate) fn validate(self) -> Result<Self, RuntimeCommandError> {
        if self.max_iterations == 0
            || self.max_input_tokens == Some(0)
            || self.max_output_tokens == Some(0)
            || self.deadline_budget_ms == Some(0)
        {
            Err(RuntimeCommandError::InvalidCommand)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Fully constructed StartTurn input; configuration and clock values are explicit.
pub struct StartTurnCommand {
    /// Idempotency identity supplied by the caller.
    pub command_id: RuntimeCommandId,
    /// Session receiving the new Turn.
    pub session_id: SessionId,
    /// Installed Agent instance.
    pub agent_instance_id: AgentInstanceId,
    /// Exact Agent definition identity.
    pub definition_id: AgentDefinitionId,
    /// Exact Agent definition revision.
    pub definition_revision: AgentDefinitionRevision,
    /// Effective snapshot SHA-256 digest.
    pub snapshot_digest: String,
    /// Trusted UTF-8 user input admitted by the Host.
    pub trusted_input: String,
    /// Limits frozen for the first Execution.
    pub limits: EffectiveRuntimeLimits,
    /// RFC 3339 observation time supplied by the Runtime clock port.
    pub recorded_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable cancellation reason admitted by C6 payload v1.
pub enum CancelReason {
    /// Explicit user cancellation.
    User,
    /// Frozen deadline reached.
    Deadline,
    /// Runtime shutdown.
    Shutdown,
    /// Operator action.
    Operator,
    /// Product policy action.
    Policy,
}

impl CancelReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Deadline => "deadline",
            Self::Shutdown => "shutdown",
            Self::Operator => "operator",
            Self::Policy => "policy",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Fully constructed cancellation command.
pub struct CancelTurnCommand {
    /// Idempotency identity supplied by the caller.
    pub command_id: RuntimeCommandId,
    /// Owning Session.
    pub session_id: SessionId,
    /// Non-terminal Turn to cancel.
    pub turn_id: TurnId,
    /// Stable cancellation reason.
    pub reason: CancelReason,
    /// Fixed durable watermark observed by the caller.
    pub requested_through_position: u64,
    /// RFC 3339 observation time supplied by the Runtime clock port.
    pub recorded_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Runtime-created identities and exact fact batch for one command.
pub struct PlannedTurn {
    /// Runtime-created Turn identity.
    pub turn_id: TurnId,
    /// Runtime-created disposable Execution identity when the command starts work.
    pub execution_id: Option<ExecutionId>,
    /// Atomic ordered fact batch.
    pub facts: Vec<FactDraft>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable command planning or execution failure.
pub enum RuntimeCommandError {
    /// A constructed command violates C6 input invariants.
    InvalidCommand,
}

impl RuntimeCommandError {
    /// Returns the stable machine-readable error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidCommand => "invalid_command",
        }
    }
}

impl fmt::Display for RuntimeCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for RuntimeCommandError {}
