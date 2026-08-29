//! Portable C5 execution facts, observations, terminals, and recovery decisions.

use serde_json::{json, Value};

use crate::{
    DispatchAttemptId, EffectReceipt, InteractionRequest, InvocationGrant, ToolInvocationId,
};

/// Fact delivered to the reducer after Runtime durability boundaries complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionFact {
    /// Runtime committed the external dispatch boundary.
    Started(DispatchAttemptId),
    /// Trustworthy successful receipt and bounded content are committed.
    Completed {
        /// Required receipt; absence is corrupt recovery state.
        receipt: Option<EffectReceipt>,
        /// Bounded model-visible JSON content.
        content: Value,
        /// Whether Runtime truncated the content deliberately.
        truncated: bool,
    },
    /// Trustworthy terminal failure evidence is committed.
    Failed {
        /// Required after Started.
        receipt: Option<EffectReceipt>,
        /// Stable safe failure code.
        code: String,
        /// Optional redacted detail.
        details: Option<String>,
        /// Optional bounded partial content.
        partial: Option<Value>,
    },
    /// Started effect lacks trustworthy terminal evidence.
    Uncertain {
        /// Stable redacted uncertainty evidence code.
        evidence: String,
    },
    /// Executor cannot enforce one requirement before Started.
    Unsupported {
        /// Stable unsupported requirement name.
        requirement: String,
    },
}

/// Model-visible governed observation outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationOutcome {
    /// Successful bounded tool content.
    Succeeded {
        /// Bounded model-visible JSON.
        content: Value,
        /// Whether Runtime truncated content deliberately.
        truncated: bool,
    },
    /// Policy or interaction rejection.
    Rejected {
        /// Stable safe rejection code.
        code: String,
        /// Optional redacted detail.
        details: Option<String>,
    },
    /// Trustworthy terminal execution failure.
    Failed {
        /// Stable safe failure code.
        code: String,
        /// Optional redacted detail.
        details: Option<String>,
        /// Optional bounded partial content.
        partial: Option<Value>,
    },
}

/// Exact model correlation and safe governed outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedObservation {
    /// Invocation identity.
    pub invocation_id: ToolInvocationId,
    /// Exact C4 digest.
    pub prepared_digest: String,
    /// Untrusted but validated model correlation.
    pub model_call_id: String,
    /// Exact tool name.
    pub tool_name: String,
    /// Safe bounded outcome.
    pub outcome: ObservationOutcome,
}

impl GovernedObservation {
    /// Returns the stable neutral model-visible JSON envelope.
    pub fn model_envelope(&self) -> Value {
        match &self.outcome {
            ObservationOutcome::Succeeded { content, truncated } => {
                json!({"status":"succeeded","content":content,"truncated":truncated})
            }
            ObservationOutcome::Rejected { code, details } => optional_envelope(
                json!({"status":"rejected","code":code}),
                "details",
                details.as_ref().map(|value| Value::String(value.clone())),
            ),
            ObservationOutcome::Failed {
                code,
                details,
                partial,
            } => {
                let value = optional_envelope(
                    json!({"status":"failed","code":code}),
                    "details",
                    details.as_ref().map(|value| Value::String(value.clone())),
                );
                optional_envelope(value, "partial", partial.clone())
            }
        }
    }
}

fn optional_envelope(mut value: Value, key: &str, item: Option<Value>) -> Value {
    if let Some(item) = item {
        value
            .as_object_mut()
            .expect("envelope is an object")
            .insert(key.to_owned(), item);
    }
    value
}

/// Stable portable reducer failure code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernedFailureCode {
    /// Grant identity, digest, tool, revision, or requirements do not bind.
    GrantMismatch,
    /// Executor cannot enforce an admitted requirement.
    RequirementUnsupported,
    /// An event conflicts with the current invocation state.
    InvocationConflict,
    /// Interaction identity, invocation, or digest conflicts.
    InteractionConflict,
    /// Receipt or reconstructed terminal state is invalid.
    CorruptRecoveryState,
    /// Model correlation is invalid.
    InvalidModelOutput,
}

/// Portable fail-closed terminal from governed reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedEffectFailure {
    /// Stable failure classification.
    pub code: GovernedFailureCode,
}

/// Suspension that terminally ends the current Execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SuspensionRequirement {
    /// A committed interaction awaits continuation.
    Interaction(InteractionRequest),
    /// An uncertain effect requires operator reconciliation.
    OperatorReconciliation {
        /// Stable redacted uncertainty evidence code.
        evidence: String,
    },
}

/// Next action produced after one durable input fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GovernedAction {
    /// Ask Runtime authorization for the exact Prepared Call again.
    Authorize,
    /// Ask Runtime execution for the exact validated grant.
    Dispatch(InvocationGrant),
    /// Return safe model-visible feedback.
    Observation(GovernedObservation),
    /// Terminally suspend the current Execution.
    Suspend(SuspensionRequirement),
    /// Fail closed without fabricated model feedback.
    Fail(GovernedEffectFailure),
    /// Idempotent duplicate produced no new action.
    None,
}

/// Portable lifecycle state after reduction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectState {
    /// Prepared and awaiting authorization.
    Prepared,
    /// Denied with a model-visible observation.
    Denied,
    /// Replacement proposal terminated the original invocation.
    Replaced,
    /// Waiting on a committed interaction.
    AwaitingInteraction,
    /// Exact grant is ready for Runtime enforcement/dispatch.
    Authorized,
    /// Runtime crossed the external dispatch boundary.
    Started,
    /// Trustworthy successful terminal.
    Completed,
    /// Fail-closed or trustworthy failed terminal.
    Failed,
    /// Started effect lacks trustworthy terminal evidence.
    Uncertain,
}

/// Durable recovery position reconstructed by Runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryPosition {
    /// Grant exists but Started does not.
    Authorized,
    /// Started exists without receipt.
    StartedNoReceipt,
    /// Receipt exists without result fact.
    ReceiptNoResult,
    /// Terminal result fact exists.
    Terminal,
}

/// Required deterministic recovery action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryDecision {
    /// Revalidate the frozen grant before first dispatch.
    RevalidateGrant,
    /// Retry the same invocation only with executor proof.
    RetrySameInvocation,
    /// Recover the executor's trustworthy journal/receipt.
    RecoverExecutorReceipt,
    /// Reconstruct result from an existing receipt.
    ReconstructFromReceipt,
    /// Return an existing terminal idempotently.
    ReturnTerminal,
    /// Suspend for operator reconciliation.
    ReconcileOperator,
}
