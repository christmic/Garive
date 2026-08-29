use std::{error::Error, fmt};

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, ExecutionId, FactDraft, SessionId,
    ToolInvocationId, TurnId,
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
/// Fixed-prefix query for a redacted durable Turn projection.
pub struct GetTurnQuery {
    /// Session expected to own the Turn.
    pub session_id: SessionId,
    /// Turn to reconstruct.
    pub turn_id: TurnId,
    /// Optional non-zero Session position freezing a historical prefix.
    pub through_position: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Durable lifecycle visible through one fixed Turn prefix.
pub enum RuntimeTurnStatus {
    /// A disposable Execution may be active or awaiting restart recovery.
    Open,
    /// The Turn requires a typed continuation.
    Suspended,
    /// The Turn completed successfully.
    Completed,
    /// A declared limit or cancellation stopped the Turn.
    Stopped,
    /// The Turn failed terminally.
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Redacted suspension binding exposed by [`RuntimeTurnView`].
pub struct RuntimeSuspensionView {
    /// Stable suspension identity required by continuation commands.
    pub suspension_id: String,
    /// Typed continuation category.
    pub kind: RuntimeSuspensionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Redacted Turn state derived solely from one verified durable prefix.
pub struct RuntimeTurnView {
    /// Session owning the Turn.
    pub session_id: SessionId,
    /// Reconstructed Turn identity.
    pub turn_id: TurnId,
    /// Exact Session fact position included by this view.
    pub through_position: u64,
    /// Latest Session transaction version observed while serving the query.
    pub observed_session_version: u64,
    /// Lifecycle at the fixed prefix.
    pub status: RuntimeTurnStatus,
    /// Latest disposable Execution created by the included prefix.
    pub execution_id: Option<ExecutionId>,
    /// Exact continuation binding when [`RuntimeTurnStatus::Suspended`].
    pub suspension: Option<RuntimeSuspensionView>,
    /// Monotonic completed/started iteration cursor at the prefix.
    pub completed_iterations: u64,
    /// Whether cancellation had been durably requested by the prefix.
    pub cancellation_requested: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Optional interaction response consumed by a continuation transaction.
pub struct InteractionContinuation {
    /// Prior Execution that requested the interaction.
    pub execution_id: ExecutionId,
    /// Effect invocation owning the interaction.
    pub tool_invocation_id: ToolInvocationId,
    /// Exact interaction identity.
    pub interaction_id: String,
    /// Prepared Call digest bound by the request.
    pub prepared_digest: String,
    /// Response-schema digest frozen by the interaction request.
    pub response_schema_digest: String,
    /// Frozen expiry policy category.
    pub expiry: InteractionExpiry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Durable interaction expiry category admitted by payload v1.
pub enum InteractionExpiry {
    /// No expiry is configured.
    None,
    /// The Turn deadline controls expiry.
    TurnDeadline,
    /// A product-policy deadline controls expiry.
    PolicyDeadline,
}

impl InteractionExpiry {
    pub(crate) fn parse(value: &str) -> Result<Self, RuntimeCommandError> {
        match value {
            "none" => Ok(Self::None),
            "turn_deadline" => Ok(Self::TurnDeadline),
            "policy_deadline" => Ok(Self::PolicyDeadline),
            _ => Err(RuntimeCommandError::CorruptLedger),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Typed input admitted for one exact suspension reason.
pub enum ContinuationInput {
    /// Schema-validated external or approval input.
    ExternalInput(String),
    /// Operator reconciliation content for one exact uncertain invocation.
    Reconciliation {
        /// Invocation closed by the prior reconciliation command.
        invocation_id: ToolInvocationId,
        /// Model-safe continuation content.
        content: String,
    },
    /// Signal that a previously unavailable resource may be attempted again.
    ResourceReady,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Suspension reason reconstructed from the durable Turn terminal.
pub enum RuntimeSuspensionKind {
    /// A governed approval response is required.
    ApprovalRequired,
    /// External product input is required.
    ExternalInputRequired,
    /// An uncertain effect requires operator evidence.
    OperatorReconciliation,
    /// A model or other frozen resource was unavailable.
    ResourceUnavailable,
    /// Partial output awaits an explicit continuation input.
    PartialOutput,
    /// An authorized child Turn has not produced an observed result.
    DelegationPending,
}

impl RuntimeSuspensionKind {
    pub(crate) fn parse(value: &str) -> Result<Self, RuntimeCommandError> {
        match value {
            "approval_required" => Ok(Self::ApprovalRequired),
            "external_input_required" => Ok(Self::ExternalInputRequired),
            "operator_reconciliation" => Ok(Self::OperatorReconciliation),
            "resource_unavailable" => Ok(Self::ResourceUnavailable),
            "partial_output" => Ok(Self::PartialOutput),
            "delegation_pending" => Ok(Self::DelegationPending),
            _ => Err(RuntimeCommandError::CorruptLedger),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Exact uncertain-effect target reconstructed for operator reconciliation.
pub struct ReconciliationTarget {
    /// Prior Execution that owns the uncertain effect.
    pub execution_id: ExecutionId,
    /// Exact uncertain invocation.
    pub invocation_id: ToolInvocationId,
    /// Prepared Call digest bound throughout the lifecycle.
    pub prepared_digest: String,
    /// Model call correlation needed for the durable observation.
    pub model_call_id: String,
    /// Whether operator evidence already closed the uncertainty.
    pub reconciled: bool,
    /// Whether the exact reconciliation observation is durable.
    pub observed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// ContinueTurn input supplied by a Host after a typed suspension.
pub struct ContinueTurnCommand {
    /// Idempotency identity supplied by the caller.
    pub command_id: RuntimeCommandId,
    /// Owning Session.
    pub session_id: SessionId,
    /// Suspended Turn to reopen.
    pub turn_id: TurnId,
    /// Suspension identity the caller observed.
    pub expected_suspension_id: String,
    /// Session version the caller observed.
    pub expected_session_version: u64,
    /// Typed, schema-validated continuation input.
    pub continuation_input: ContinuationInput,
    /// Optional interaction binding consumed before reopening.
    pub interaction: Option<InteractionContinuation>,
    /// RFC 3339 observation time supplied by the Runtime clock port.
    pub recorded_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Durable suspended state reconstructed from a fixed Ledger prefix.
pub struct SuspendedTurnState {
    /// Session owning the fixed durable prefix.
    pub session_id: SessionId,
    /// Session version at the fixed prefix.
    pub session_version: u64,
    /// Suspended Turn identity.
    pub turn_id: TurnId,
    /// Exact suspension identity.
    pub suspension_id: String,
    /// Exact reason constraining the accepted continuation input kind.
    pub suspension_kind: RuntimeSuspensionKind,
    /// Pending interaction binding, when the suspension requested one.
    pub interaction: Option<InteractionContinuation>,
    /// Uncertain effect binding, when operator reconciliation is required.
    pub reconciliation: Option<ReconciliationTarget>,
    /// Installed Agent instance retained from Turn start.
    pub agent_instance_id: AgentInstanceId,
    /// Exact definition identity retained from Turn start.
    pub definition_id: AgentDefinitionId,
    /// Exact definition revision retained from Turn start.
    pub definition_revision: AgentDefinitionRevision,
    /// Effective snapshot digest retained from Turn start.
    pub snapshot_digest: String,
    /// Original trusted-input digest retained from Turn start.
    pub trusted_input_digest: String,
    /// Fixed Ledger position used to rebuild the new cursor.
    pub through_position: u64,
    /// Cumulative completed iteration count.
    pub completed_iterations: u64,
    /// Number of prior Runtime recovery executions.
    pub recovery_ordinal: u64,
    /// Limits frozen for the fresh Execution.
    pub limits: EffectiveRuntimeLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Conclusive operator decision for one uncertain effect.
pub enum ReconciliationDecision {
    /// Evidence confirms the effect completed.
    Completed {
        /// Redacted model-safe observation of the confirmed result.
        model_observation: String,
    },
    /// Evidence confirms the effect failed.
    Failed {
        /// Redacted model-safe observation of the confirmed failure.
        model_observation: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Idempotent command closing one uncertain effect without redispatch.
pub struct ReconcileInvocationCommand {
    /// Idempotency identity supplied by the caller.
    pub command_id: RuntimeCommandId,
    /// Owning Session.
    pub session_id: SessionId,
    /// Suspended Turn.
    pub turn_id: TurnId,
    /// Exact uncertain invocation.
    pub invocation_id: ToolInvocationId,
    /// Suspension identity observed by the caller.
    pub expected_suspension_id: String,
    /// Session version observed by the caller.
    pub expected_session_version: u64,
    /// Redacted durable operator evidence.
    pub operator_evidence: String,
    /// Conclusive outcome and model-safe observation.
    pub decision: ReconciliationDecision,
    /// RFC 3339 observation time supplied by the Runtime clock port.
    pub recorded_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Inputs for atomically abandoning a lost Kernel and starting its replacement.
pub struct RecoveryRestartCommand {
    /// Stable internal recovery operation identity.
    pub recovery_id: RuntimeCommandId,
    /// Still-open owning Turn.
    pub turn_id: TurnId,
    /// Lost active Execution to classify as abandoned.
    pub lost_execution_id: ExecutionId,
    /// Effective snapshot digest retained from Turn start.
    pub snapshot_digest: String,
    /// Last fixed Ledger position proven safe for cursor reconstruction.
    pub last_safe_position: u64,
    /// Cumulative completed iteration count.
    pub completed_iterations: u64,
    /// Non-zero ordinal assigned to the replacement Execution.
    pub recovery_ordinal: u64,
    /// Frozen effective limits.
    pub limits: EffectiveRuntimeLimits,
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
    /// A command identity was previously committed with different semantics.
    CommandConflict,
    /// Optimistic Session version is stale.
    ConcurrentModification,
    /// Expected suspension or Turn identity does not match durable state.
    ContinuationMismatch,
    /// Durable facts do not describe a currently suspended Turn.
    TurnNotResumable,
    /// Verified storage rows contain impossible Runtime referents or values.
    CorruptLedger,
    /// Core supplied internally inconsistent terminal evidence.
    InvariantViolation,
    /// Required durable storage was unavailable.
    DurabilityFailure,
}

impl RuntimeCommandError {
    /// Returns the stable machine-readable error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidCommand => "invalid_command",
            Self::CommandConflict => "command_conflict",
            Self::ConcurrentModification => "concurrent_modification",
            Self::ContinuationMismatch => "continuation_mismatch",
            Self::TurnNotResumable => "turn_not_resumable",
            Self::CorruptLedger => "corrupt_ledger",
            Self::InvariantViolation => "invariant_violation",
            Self::DurabilityFailure => "durability_failure",
        }
    }
}

impl fmt::Display for RuntimeCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for RuntimeCommandError {}
