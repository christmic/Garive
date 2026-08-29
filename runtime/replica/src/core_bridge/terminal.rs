use std::fmt::Write;

use garive_core::{
    AgentFailureReason, AgentOutcome, ExecutionReport, StopReason, SuspensionReason, UsageSummary,
};
use garive_ledger::{CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, TurnId};
use garive_llm::{MediaKind, ModelItem, ReasoningContent, TokenCount};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::RuntimeCommandError;

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
                json!({"response":response,"usage":usage}),
                json!({"execution_id":context.execution_id.as_str(),"response":response,"cumulative_usage":usage}),
            )
        }
        AgentOutcome::Suspended {
            reason,
            partial_items,
            ..
        } => {
            let continuation = content(partial_items)?;
            let reason = suspension_reason(*reason);
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
            (
                "execution.suspended",
                "turn.suspended",
                json!({"suspension_id":suspension_id,"reason":reason,"continuation":continuation,"usage":usage}),
                json!({"suspension_id":suspension_id,"execution_id":context.execution_id.as_str(),"reason":reason,"continuation":continuation,"cumulative_usage":usage}),
            )
        }
        AgentOutcome::Stopped { reason } => {
            let reason = stop_reason(*reason);
            (
                "execution.stopped",
                "turn.stopped",
                json!({"reason":reason,"usage":usage}),
                json!({"execution_id":context.execution_id.as_str(),"reason":reason,"cumulative_usage":usage}),
            )
        }
        AgentOutcome::Failed { reason } => {
            let reason = failure_reason(*reason);
            (
                "execution.failed",
                "turn.failed",
                json!({"reason":reason,"usage":usage}),
                json!({"execution_id":context.execution_id.as_str(),"reason":reason,"cumulative_usage":usage}),
            )
        }
    };
    Ok(vec![
        fact(context, execution_kind, true, execution_payload)?,
        fact(context, turn_kind, false, turn_payload)?,
    ])
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

fn content(items: &[ModelItem]) -> Result<Value, RuntimeCommandError> {
    let values = items.iter().map(model_item).collect::<Vec<_>>();
    let canonical = CanonicalPayload::from_value(&Value::Array(values))
        .map_err(|_| RuntimeCommandError::InvariantViolation)?;
    Ok(json!({"digest":canonical.sha256(),"inline_utf8":canonical.as_json()}))
}

fn model_item(item: &ModelItem) -> Value {
    match item {
        ModelItem::Text { text } => json!({"kind":"text","text":text}),
        ModelItem::Refusal { text } => json!({"kind":"refusal","text":text}),
        ModelItem::Reasoning { content } => match content {
            ReasoningContent::ModelVisible(text) => {
                json!({"kind":"reasoning","visibility":"model_visible","value":text})
            }
            ReasoningContent::OpaqueReference(reference) => {
                json!({"kind":"reasoning","visibility":"opaque_reference","value":reference})
            }
        },
        ModelItem::ToolIntent {
            model_call_id,
            tool_name,
            arguments_json,
        } => {
            json!({"kind":"tool_intent","model_call_id":model_call_id,"tool_name":tool_name,"arguments_json":arguments_json})
        }
        ModelItem::ToolObservation {
            model_call_id,
            result_json,
        } => {
            json!({"kind":"tool_observation","model_call_id":model_call_id,"result_json":result_json})
        }
        ModelItem::MediaReference {
            media_kind,
            reference,
        } => {
            json!({"kind":"media_reference","media_kind":media_kind_value(media_kind),"reference":reference})
        }
    }
}

fn media_kind_value(kind: &MediaKind) -> Value {
    match kind {
        MediaKind::Image => json!("image"),
        MediaKind::Audio => json!("audio"),
        MediaKind::Video => json!("video"),
        MediaKind::File => json!("file"),
        MediaKind::Other(value) => json!({"other":value}),
    }
}

const fn suspension_reason(reason: SuspensionReason) -> &'static str {
    match reason {
        SuspensionReason::PartialOutput => "partial_output",
        SuspensionReason::ResourceUnavailable => "resource_unavailable",
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

fn digest(value: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(value) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}
