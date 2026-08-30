use std::{error::Error, fmt, future::Future, pin::Pin};

use garive_ledger::{ExecutionId, SessionId, TurnId};
use garive_tools::{
    EffectReceipt, ExecutionFact, ExecutionRequirements, InteractionKind, InvocationGrant,
    PreparedToolCall, ToolInvocationId,
};
use serde_json::Value;

use crate::{SafetyDecisionV1, SafetyRequestV1, SandboxBindingV1};

/// Frozen durable ownership and time values for one effect-capable Execution.
pub struct GovernedEffectConfig {
    /// Session receiving every effect fact.
    pub session_id: SessionId,
    /// Session version immediately before this port's first commit.
    pub expected_session_version: u64,
    /// Latest committed Session position before this port's first commit.
    pub initial_through_position: u64,
    /// Turn owning every effect.
    pub turn_id: TurnId,
    /// Disposable Execution owning every effect.
    pub execution_id: ExecutionId,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Full exact request presented to the frozen authority implementation.
pub struct AuthorityRequest<'a> {
    /// Runtime-owned invocation identity.
    pub invocation_id: &'a ToolInvocationId,
    /// Exact authority-free prepared call.
    pub prepared: &'a PreparedToolCall,
}

/// Runtime-neutral authority decision; Runtime allocates all resulting IDs.
pub enum AuthorityDecision {
    /// Permit equal-or-stricter requirements under exact policy evidence.
    Approve {
        /// Requirements granted by authority.
        granted_requirements: ExecutionRequirements,
        /// Digest of frozen executor constraints.
        constraints_digest: String,
        /// Exact authority policy revision.
        authority_revision: String,
    },
    /// Reject the exact prepared call.
    Deny {
        /// Optional bounded safe details.
        safe_details: Option<String>,
    },
    /// Require a newly prepared replacement rather than granting this call.
    ReplacementRequired,
    /// Require a durable interaction before authority is reconsidered.
    InteractionRequired {
        /// Interaction family.
        kind: InteractionKind,
        /// Redacted structured prompt.
        prompt: Value,
        /// Portable response schema.
        response_schema: Value,
        /// Stable expiry policy code.
        expiry_code: String,
    },
}

/// Asynchronous authority result.
pub type AuthorityFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AuthorityDecision, GovernedRuntimePortError>> + Send + 'a>>;

/// Frozen authority boundary for exact prepared calls.
pub trait AuthorityPort: Send {
    /// Decides without allocating Runtime identities or crossing execution.
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a>;
}

/// Asynchronous F0 Safety policy result for one exact request.
pub type SafetyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SafetyDecisionV1, GovernedRuntimePortError>> + Send + 'a>>;

/// Runtime policy boundary that cannot allocate a C5 grant or select an executor.
pub trait SafetyPort: Send {
    /// Evaluates one immutable request without crossing an external-effect boundary.
    fn decide<'a>(&'a mut self, request: &'a SafetyRequestV1) -> SafetyFuture<'a>;
}

/// Exact inputs used to select and prove one concrete Sandbox binding.
pub struct SandboxAdmissionRequest<'a> {
    /// Safety request whose exact resources must be enforceable.
    pub safety_request: &'a SafetyRequestV1,
    /// Safety decision committed before grant conversion.
    pub decision: &'a SafetyDecisionV1,
    /// C5 grant derived from the exact Allow constraints.
    pub grant: &'a InvocationGrant,
}

/// Concrete preflight evidence returned without dispatching the tool.
pub struct SandboxAdmission {
    /// Immutable OS/executor/workspace binding.
    pub binding: SandboxBindingV1,
    /// Digest of post-narrowing concrete limits.
    pub effective_limits_digest: String,
    /// Runtime-owned stable preflight identity.
    pub preflight_id: String,
    /// Executor dispatch-attempt identity.
    pub dispatch_attempt_id: String,
}

/// Runtime Sandbox broker boundary; selection and proof occur before Started.
pub trait SandboxAdmissionPort: Send {
    /// Selects an enforceable binding but must not dispatch the invocation.
    fn admit(
        &mut self,
        request: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, GovernedRuntimePortError>;
}

/// Executor identity and dispatch attempt proven enforceable before Started.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedExecution {
    /// Stable executor identity.
    pub executor_id: String,
    /// Exact executor revision.
    pub executor_revision: String,
    /// Runtime/executor dispatch-attempt identity.
    pub dispatch_attempt_id: String,
}

/// Exact authorized command delivered only after `effect.started` commits.
pub struct ExecutorDispatch<'a> {
    /// Runtime-owned invocation identity.
    pub invocation_id: &'a ToolInvocationId,
    /// Exact prepared call.
    pub prepared: &'a PreparedToolCall,
    /// Exact frozen grant.
    pub grant: &'a InvocationGrant,
    /// Preflight-selected executor binding.
    pub execution: &'a PreparedExecution,
    /// Runtime-owned receipt identity the executor must bind if terminal.
    pub receipt_id: &'a str,
}

/// Loss after Started without trustworthy terminal evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutorDispatchError {
    /// Dispatch crossed but no receipt was recovered.
    StartedWithoutReceipt,
    /// Executor state cannot be proven.
    ExecutorStateUnknown,
    /// Returned receipt or result binding failed validation.
    ReceiptInvalid,
}

/// Asynchronous executor terminal result after dispatch.
pub type ExecutorFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ExecutionFact, ExecutorDispatchError>> + Send + 'a>>;

/// Two-phase executor boundary preserving the before-Started durability cut.
pub trait ExecutorPort: Send {
    /// Selects an executor and proves requirement enforcement without dispatch.
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String>;

    /// Crosses the external boundary only after Runtime committed Started.
    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a>;
}

/// Sanitized failure of Runtime effect composition.
#[derive(Debug)]
pub enum GovernedRuntimePortError {
    /// Durable ledger operation failed.
    Ledger(crate::SqliteLedgerError),
    /// Supplied identities, payloads, or bindings violate the contract.
    InvalidBinding,
    /// Authority dependency failed before any dispatch.
    AuthorityUnavailable,
}

impl fmt::Display for GovernedRuntimePortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ledger(error) => write!(formatter, "governed ledger operation failed: {error}"),
            Self::InvalidBinding => formatter.write_str("invalid governed effect binding"),
            Self::AuthorityUnavailable => formatter.write_str("authority unavailable"),
        }
    }
}

impl Error for GovernedRuntimePortError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ledger(error) => Some(error),
            _ => None,
        }
    }
}

impl From<crate::SqliteLedgerError> for GovernedRuntimePortError {
    fn from(value: crate::SqliteLedgerError) -> Self {
        Self::Ledger(value)
    }
}

/// Extracts a terminal receipt from an executor fact when present.
pub(crate) fn receipt(fact: &ExecutionFact) -> Option<&EffectReceipt> {
    match fact {
        ExecutionFact::Completed { receipt, .. } | ExecutionFact::Failed { receipt, .. } => {
            receipt.as_ref()
        }
        _ => None,
    }
}
