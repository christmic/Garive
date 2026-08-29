use std::collections::BTreeSet;

use garive_tools::validate_portable_value;
use serde::Serialize;

use crate::values::{sha256, valid_digest, valid_id};
use crate::{
    settle_delegation_budget, ContentBinding, DelegationBudgetSettlement, DelegationConsumption,
    DelegationError, DelegationErrorCode, DelegationIntent, DelegationUsage, FactReference,
};

/// Stable child terminal reason admitted into a bounded parent observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildTerminalReason {
    /// Child reached its iteration limit.
    IterationLimit,
    /// Child reached a token limit.
    TokenLimit,
    /// Child exhausted its deadline.
    Deadline,
    /// Child observed cancellation.
    Cancelled,
    /// Required child resource was unavailable.
    ResourceUnavailable,
    /// Child input was invalid.
    InvalidInput,
    /// Child model output was invalid.
    InvalidModelOutput,
    /// Required child capability was unavailable.
    RequiredCapabilityUnavailable,
    /// A child execution port failed.
    PortFailure,
    /// Child Core detected an invariant violation.
    InvariantViolation,
    /// Child durable terminal commit failed.
    DurabilityFailure,
    /// Child recovery state was corrupt.
    CorruptRecoveryState,
}

impl ChildTerminalReason {
    /// Returns the exact stable wire name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::IterationLimit => "iteration_limit",
            Self::TokenLimit => "token_limit",
            Self::Deadline => "deadline",
            Self::Cancelled => "cancelled",
            Self::ResourceUnavailable => "resource_unavailable",
            Self::InvalidInput => "invalid_input",
            Self::InvalidModelOutput => "invalid_model_output",
            Self::RequiredCapabilityUnavailable => "required_capability_unavailable",
            Self::PortFailure => "port_failure",
            Self::InvariantViolation => "invariant_violation",
            Self::DurabilityFailure => "durability_failure",
            Self::CorruptRecoveryState => "corrupt_recovery_state",
        }
    }
}

/// Non-completion terminal category of the child Turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalOutcomeKind {
    /// Child stopped under a normal policy boundary.
    Stopped,
    /// Child failed under a stable failure boundary.
    Failed,
}

/// Portable child evidence exposed to the parent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DelegationOutcome {
    /// Schema-valid bounded child completion.
    Completed {
        /// Exact redacted result content.
        content: ContentBinding,
        /// Ordered exact child evidence references.
        evidence: Vec<FactReference>,
    },
    /// Child stopped without a completion value.
    Stopped {
        /// Exact child stop reason.
        reason: ChildTerminalReason,
    },
    /// Child failed without a completion value.
    Failed {
        /// Exact child failure reason.
        reason: ChildTerminalReason,
    },
}

/// Exact child/result identities and terminal accounting evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DelegationResultContext {
    /// Stable result identity.
    pub result_id: String,
    /// Logical delegation identity.
    pub delegation_id: String,
    /// Exact authority grant identity.
    pub grant_id: String,
    /// Exact child Agent instance.
    pub child_agent_instance_id: String,
    /// Exact child Turn.
    pub child_turn_id: String,
    /// Child effective snapshot digest.
    pub child_snapshot_digest: String,
    /// Conservative child token evidence.
    pub usage: DelegationUsage,
    /// Exact finite child lifecycle consumption.
    pub consumption: DelegationConsumption,
}

/// Validated terminal child result and budget settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationResult {
    context: DelegationResultContext,
    outcome: DelegationOutcome,
    settlement: DelegationBudgetSettlement,
}

impl DelegationResult {
    /// Returns exact result context and accounting evidence.
    pub const fn context(&self) -> &DelegationResultContext {
        &self.context
    }
    /// Returns bounded outcome visible to the parent.
    pub const fn outcome(&self) -> &DelegationOutcome {
        &self.outcome
    }
    /// Returns conservative terminal charge and releasable reservation.
    pub const fn settlement(&self) -> DelegationBudgetSettlement {
        self.settlement
    }
    /// Returns RFC 8785 canonical JSON binding every governed result field.
    pub fn result_binding(&self) -> Result<ContentBinding, DelegationError> {
        let value = serde_json::json!({
            "contract":"garive.delegation-result","version":1,
            "result_id":self.context.result_id,"delegation_id":self.context.delegation_id,
            "grant_id":self.context.grant_id,
            "child_agent_instance_id":self.context.child_agent_instance_id,
            "child_turn_id":self.context.child_turn_id,
            "child_snapshot_digest":self.context.child_snapshot_digest,
            "outcome":self.outcome,"usage":self.context.usage,
            "consumption":self.context.consumption,
        });
        let bytes = serde_jcs::to_vec(&value).map_err(|_| invalid())?;
        let text = String::from_utf8(bytes).map_err(|_| invalid())?;
        Ok(ContentBinding::from_inline(text))
    }
}

/// Validates one completed child value against the frozen schema and bounds.
pub fn complete_delegation_result(
    intent: &DelegationIntent,
    context: DelegationResultContext,
    content: ContentBinding,
    resolved_content_utf8: &str,
    evidence: Vec<FactReference>,
) -> Result<DelegationResult, DelegationError> {
    let settlement = validate_context(intent, &context)?;
    if sha256(resolved_content_utf8.as_bytes()) != content.digest()
        || resolved_content_utf8.len() as u64 > intent.budget().max_result_bytes
        || evidence.len() as u64 > intent.budget().max_result_evidence
        || !unique_evidence(&evidence)
    {
        return Err(invalid());
    }
    let schema: serde_json::Value =
        serde_json::from_str(intent.result_schema().inline_utf8().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    let value: serde_json::Value =
        serde_json::from_str(resolved_content_utf8).map_err(|_| schema_mismatch())?;
    if !validate_portable_value(&schema, &value)
        .map_err(|_| schema_mismatch())?
        .is_empty()
    {
        return Err(schema_mismatch());
    }
    Ok(DelegationResult {
        context,
        outcome: DelegationOutcome::Completed { content, evidence },
        settlement,
    })
}

/// Validates one stopped or failed child terminal without inventing content.
pub fn terminal_delegation_result(
    intent: &DelegationIntent,
    context: DelegationResultContext,
    kind: TerminalOutcomeKind,
    reason: ChildTerminalReason,
) -> Result<DelegationResult, DelegationError> {
    let settlement = validate_context(intent, &context)?;
    let outcome = match kind {
        TerminalOutcomeKind::Stopped => DelegationOutcome::Stopped { reason },
        TerminalOutcomeKind::Failed => DelegationOutcome::Failed { reason },
    };
    Ok(DelegationResult {
        context,
        outcome,
        settlement,
    })
}

fn validate_context(
    intent: &DelegationIntent,
    context: &DelegationResultContext,
) -> Result<DelegationBudgetSettlement, DelegationError> {
    if context.delegation_id != intent.delegation_id()
        || !valid_id(&context.result_id)
        || !valid_id(&context.grant_id)
        || !valid_id(&context.child_agent_instance_id)
        || !valid_id(&context.child_turn_id)
        || !valid_digest(&context.child_snapshot_digest)
    {
        return Err(invalid());
    }
    settle_delegation_budget(intent.budget(), context.consumption, context.usage)
}

fn unique_evidence(values: &[FactReference]) -> bool {
    let mut unique = BTreeSet::new();
    values.iter().all(|value| unique.insert(value.clone()))
}

const fn invalid() -> DelegationError {
    DelegationError::new(DelegationErrorCode::InvalidDelegation)
}
const fn schema_mismatch() -> DelegationError {
    DelegationError::new(DelegationErrorCode::ResultSchemaMismatch)
}
