use garive_core::{
    AgentFailureReason, AgentOutcome, ExecutionReport, GovernedSuspensionBinding, StopReason,
    SuspensionReason, UsageSummary,
};
use garive_ledger::{CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, TurnId};
use garive_llm::TokenCount;
use serde_json::{json, Value};

use crate::RuntimeCommandError;

use super::encoding::{content, digest, value_content};

/// Immutable envelope values needed to map one Core terminal proposal.
pub struct CoreTerminalContext {
    /// Durable Turn closed or suspended by the report.
    pub turn_id: TurnId,
    /// Disposable Execution that produced the report.
    pub execution_id: ExecutionId,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Maps one Core report into the atomic execution/Turn terminal fact pair.
pub fn plan_core_terminal(
    context: &CoreTerminalContext,
    report: &ExecutionReport,
) -> Result<Vec<FactDraft>, RuntimeCommandError> {
    chrono::DateTime::parse_from_rfc3339(&context.recorded_at)
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let usage = usage(&report.usage);
    let (execution_kind, turn_kind, execution_payload, turn_payload) = match &report.outcome {
        AgentOutcome::Completed {
            response_items,
            usage: outcome_usage,
        } => {
            if outcome_usage != &report.usage {
                return Err(RuntimeCommandError::InvariantViolation);
            }
            let response = content(response_items)?;
            (
                "execution.completed",
                "turn.completed",
                json!({"response":response,"usage":usage,"completed_iterations":report.completed_iterations}),
                json!({"execution_id":context.execution_id.as_str(),"response":response,"cumulative_usage":usage}),
            )
        }
        AgentOutcome::Suspended {
            reason,
            partial_items,
            governed_binding,
            ..
        } => {
            let (continuation, suspension_id) =
                suspension_content(context, *reason, partial_items, governed_binding.as_ref())?;
            let reason = suspension_reason(*reason);
            (
                "execution.suspended",
                "turn.suspended",
                json!({"suspension_id":suspension_id,"reason":reason,"continuation":continuation,"usage":usage,"completed_iterations":report.completed_iterations}),
                json!({"suspension_id":suspension_id,"execution_id":context.execution_id.as_str(),"reason":reason,"continuation":continuation,"cumulative_usage":usage}),
            )
        }
        AgentOutcome::Stopped { reason } => {
            let reason = stop_reason(*reason);
            (
                "execution.stopped",
                "turn.stopped",
                json!({"reason":reason,"usage":usage,"completed_iterations":report.completed_iterations}),
                json!({"execution_id":context.execution_id.as_str(),"reason":reason,"cumulative_usage":usage}),
            )
        }
        AgentOutcome::Failed { reason } => {
            let reason = failure_reason(*reason);
            (
                "execution.failed",
                "turn.failed",
                json!({"reason":reason,"usage":usage,"completed_iterations":report.completed_iterations}),
                json!({"execution_id":context.execution_id.as_str(),"reason":reason,"cumulative_usage":usage}),
            )
        }
    };
    Ok(vec![
        fact(context, execution_kind, true, execution_payload)?,
        fact(context, turn_kind, false, turn_payload)?,
    ])
}

fn suspension_content(
    context: &CoreTerminalContext,
    reason: SuspensionReason,
    partial_items: &[garive_llm::ModelItem],
    binding: Option<&GovernedSuspensionBinding>,
) -> Result<(Value, String), RuntimeCommandError> {
    let governed_reason = matches!(
        reason,
        SuspensionReason::ApprovalRequired
            | SuspensionReason::ExternalInputRequired
            | SuspensionReason::OperatorReconciliation
    );
    match binding {
        Some(GovernedSuspensionBinding::Interaction {
            suspension_id,
            interaction_id,
            invocation_id,
            prepared_digest,
        }) if matches!(
            reason,
            SuspensionReason::ApprovalRequired | SuspensionReason::ExternalInputRequired
        ) =>
        {
            Ok((
                value_content(&json!({
                    "kind":"interaction","interaction_id":interaction_id,
                    "invocation_id":invocation_id,"prepared_digest":prepared_digest,
                }))?,
                suspension_id.clone(),
            ))
        }
        Some(GovernedSuspensionBinding::OperatorReconciliation {
            suspension_id,
            invocation_id,
            prepared_digest,
        }) if reason == SuspensionReason::OperatorReconciliation => Ok((
            value_content(&json!({
                "kind":"operator_reconciliation","invocation_id":invocation_id,
                "prepared_digest":prepared_digest,
            }))?,
            suspension_id.clone(),
        )),
        Some(_) | None if governed_reason => Err(RuntimeCommandError::InvariantViolation),
        Some(_) => Err(RuntimeCommandError::InvariantViolation),
        None => {
            let continuation = content(partial_items)?;
            let suspension_id = format!(
                "suspension-{}",
                digest(
                    format!(
                        "{}:{}",
                        context.execution_id.as_str(),
                        continuation["digest"]
                    )
                    .as_bytes()
                )
            );
            Ok((continuation, suspension_id))
        }
    }
}

fn fact(
    context: &CoreTerminalContext,
    kind: &str,
    execution: bool,
    payload: Value,
) -> Result<FactDraft, RuntimeCommandError> {
    let id = digest(format!("{}:{kind}", context.execution_id.as_str()).as_bytes());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{id}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: execution.then(|| context.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn usage(summary: &UsageSummary) -> Value {
    json!({
        "input_tokens": token_count(summary.input_tokens),
        "output_tokens": token_count(summary.output_tokens),
        "source": if summary.estimated { "estimated" } else { "provider_reported" },
    })
}

fn token_count(count: TokenCount) -> Value {
    match count {
        TokenCount::Known(value) => json!({"kind":"known","value":value}),
        TokenCount::Unknown => json!({"kind":"unknown"}),
    }
}

const fn suspension_reason(reason: SuspensionReason) -> &'static str {
    match reason {
        SuspensionReason::ApprovalRequired => "approval_required",
        SuspensionReason::ExternalInputRequired => "external_input_required",
        SuspensionReason::OperatorReconciliation => "operator_reconciliation",
        SuspensionReason::PartialOutput => "partial_output",
        SuspensionReason::ResourceUnavailable => "resource_unavailable",
        SuspensionReason::DelegationPending => "delegation_pending",
    }
}

const fn stop_reason(reason: StopReason) -> &'static str {
    match reason {
        StopReason::IterationLimit => "iteration_limit",
        StopReason::TokenLimit => "token_limit",
        StopReason::Deadline => "deadline",
        StopReason::Cancelled => "cancelled",
        StopReason::ResourceUnavailable => "resource_unavailable",
    }
}

const fn failure_reason(reason: AgentFailureReason) -> &'static str {
    match reason {
        AgentFailureReason::InvalidInput => "invalid_input",
        AgentFailureReason::InvalidModelOutput => "invalid_model_output",
        AgentFailureReason::RequiredCapabilityUnavailable => "required_capability_unavailable",
        AgentFailureReason::PortFailure => "port_failure",
        AgentFailureReason::InvariantViolation => "invariant_violation",
    }
}
