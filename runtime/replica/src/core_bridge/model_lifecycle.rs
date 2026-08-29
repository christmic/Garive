use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, ModelRequestId, TurnId,
};
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelRequest, ModelStopReason, ModelUsage, RejectionKind,
    TokenCount, UnavailableKind, UsageSource,
};
use serde_json::{json, Map, Value};

use crate::RuntimeCommandError;

use super::encoding::{canonical_model_request_digest, content, digest, text_content};

/// Frozen Runtime policy and ownership for one model lifecycle.
pub struct ModelLifecycleContext {
    /// Durable Turn owning the request.
    pub turn_id: TurnId,
    /// Disposable Execution owning the request.
    pub execution_id: ExecutionId,
    /// Runtime deployment identity selected behind the neutral target.
    pub deployment_id: String,
    /// Frozen recovery-policy revision.
    pub recovery_policy_revision: String,
    /// Non-zero dispatch-attempt bound.
    pub max_attempts: u64,
    /// RFC 3339 observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Bound identity, digest, and durable preparation fact for one neutral request.
pub struct PreparedModelRequest {
    /// Typed logical request identity.
    pub request_id: ModelRequestId,
    /// C6 canonical neutral request digest.
    pub request_digest: String,
    /// Fact that must commit before dispatch starts.
    pub fact: FactDraft,
}

/// Stable uncertainty reason used when dispatch lacks a normalized terminal.
pub enum RuntimeModelUncertainReason {
    /// Runtime process was lost after dispatch.
    RuntimeLost,
    /// Transport state was lost after dispatch.
    TransportLost,
    /// Provider state cannot be proven from available evidence.
    ProviderStateUnknown,
}

/// Plans `model.prepared` from the exact frozen neutral request.
pub fn plan_model_prepared(
    context: &ModelLifecycleContext,
    request: &ModelRequest,
) -> Result<PreparedModelRequest, RuntimeCommandError> {
    validate_context(context)?;
    let request_id = ModelRequestId::try_from(request.request_id.as_str())
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let request_digest = canonical_model_request_digest(request)?;
    let fact = fact(
        context,
        &request_id,
        "model.prepared",
        json!({
            "request_digest":request_digest,
            "capability_target":request.target_id.as_str(),
            "deployment_id":context.deployment_id,
            "recovery_policy_revision":context.recovery_policy_revision,
            "max_attempts":context.max_attempts,
        }),
    )?;
    Ok(PreparedModelRequest {
        request_id,
        request_digest,
        fact,
    })
}

/// Plans `model.started` immediately before one provider dispatch attempt.
pub fn plan_model_started(
    context: &ModelLifecycleContext,
    prepared: &PreparedModelRequest,
    dispatch_attempt_id: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    if dispatch_attempt_id.is_empty() {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    fact(
        context,
        &prepared.request_id,
        "model.started",
        json!({"request_digest":prepared.request_digest,"dispatch_attempt_id":dispatch_attempt_id}),
    )
}

/// Plans the exact normalized model terminal returned by the neutral port.
pub fn plan_model_terminal(
    context: &ModelLifecycleContext,
    prepared: &PreparedModelRequest,
    outcome: &InvokeOutcome,
) -> Result<FactDraft, RuntimeCommandError> {
    let (kind, payload) = match outcome {
        InvokeOutcome::Completed {
            items,
            usage,
            stop_reason,
        } => (
            "model.completed",
            json!({
                "request_digest":prepared.request_digest,"stop_reason":stop_reason_value(stop_reason),
                "items":content(items)?,"usage":usage_value(usage),
            }),
        ),
        InvokeOutcome::Rejected {
            kind,
            sanitized_evidence,
        } => {
            let mut value = Map::from_iter([
                ("request_digest".into(), json!(prepared.request_digest)),
                ("kind".into(), json!(rejection_value(*kind))),
            ]);
            if !sanitized_evidence.is_empty() {
                value.insert("evidence".into(), text_content(sanitized_evidence)?);
            }
            ("model.rejected", Value::Object(value))
        }
        InvokeOutcome::Interrupted {
            kind,
            partial_items,
            usage,
        } => (
            "model.interrupted",
            json!({
                "request_digest":prepared.request_digest,"kind":interruption_value(*kind),
                "partial_items":content(partial_items)?,"usage":usage_value(usage),
            }),
        ),
        InvokeOutcome::Unavailable { kind, retry_after } => {
            let mut value = Map::from_iter([
                ("request_digest".into(), json!(prepared.request_digest)),
                ("kind".into(), json!(unavailable_value(*kind))),
            ]);
            if let Some(delay) = retry_after {
                let millis = u64::try_from(delay.as_millis())
                    .map_err(|_| RuntimeCommandError::InvariantViolation)?;
                value.insert("retry_after_ms".into(), json!(millis));
            }
            ("model.unavailable", Value::Object(value))
        }
    };
    fact(context, &prepared.request_id, kind, payload)
}

/// Plans an explicit terminal when dispatch cannot yield a normalized outcome.
pub fn plan_model_uncertain(
    context: &ModelLifecycleContext,
    prepared: &PreparedModelRequest,
    reason: RuntimeModelUncertainReason,
) -> Result<FactDraft, RuntimeCommandError> {
    fact(
        context,
        &prepared.request_id,
        "model.uncertain",
        json!({
            "request_digest":prepared.request_digest,
            "reason":match reason { RuntimeModelUncertainReason::RuntimeLost => "runtime_lost", RuntimeModelUncertainReason::TransportLost => "transport_lost", RuntimeModelUncertainReason::ProviderStateUnknown => "provider_state_unknown" },
        }),
    )
}

fn fact(
    context: &ModelLifecycleContext,
    request: &ModelRequestId,
    kind: &str,
    payload: Value,
) -> Result<FactDraft, RuntimeCommandError> {
    let id = digest(format!("{}:{kind}", request.as_str()).as_bytes());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{id}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: Some(context.execution_id.clone()),
        model_request_id: Some(request.clone()),
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn validate_context(value: &ModelLifecycleContext) -> Result<(), RuntimeCommandError> {
    if value.deployment_id.is_empty()
        || value.recovery_policy_revision.is_empty()
        || value.max_attempts == 0
        || chrono::DateTime::parse_from_rfc3339(&value.recorded_at).is_err()
    {
        Err(RuntimeCommandError::InvalidCommand)
    } else {
        Ok(())
    }
}

fn usage_value(value: &ModelUsage) -> Value {
    let mut output = Map::from_iter([
        ("input_tokens".into(), count(value.input_tokens)),
        ("output_tokens".into(), count(value.output_tokens)),
        (
            "source".into(),
            json!(match value.source {
                UsageSource::ProviderReported => "provider_reported",
                UsageSource::Estimated => "estimated",
            }),
        ),
    ]);
    if let Some(value) = value.cache_read_tokens {
        output.insert("cache_read_tokens".into(), count(value));
    }
    if let Some(value) = value.cache_write_tokens {
        output.insert("cache_write_tokens".into(), count(value));
    }
    Value::Object(output)
}

fn count(value: TokenCount) -> Value {
    match value {
        TokenCount::Known(value) => json!({"kind":"known","value":value}),
        TokenCount::Unknown => json!({"kind":"unknown"}),
    }
}
fn stop_reason_value(value: &ModelStopReason) -> &'static str {
    match value {
        ModelStopReason::EndTurn => "end_turn",
        ModelStopReason::ToolUse => "tool_use",
        ModelStopReason::StopSequence => "stop_sequence",
        ModelStopReason::PauseTurn => "pause_turn",
        ModelStopReason::Refusal => "refusal",
        ModelStopReason::Other(_) => "other",
    }
}
const fn rejection_value(value: RejectionKind) -> &'static str {
    match value {
        RejectionKind::ContextOverflow => "context_overflow",
        RejectionKind::Authentication => "authentication",
        RejectionKind::ContentPolicy => "content_policy",
    }
}
const fn interruption_value(value: InterruptionKind) -> &'static str {
    match value {
        InterruptionKind::Cancelled => "cancelled",
        InterruptionKind::OutputLimit => "output_limit",
        InterruptionKind::Transport => "transport",
    }
}
const fn unavailable_value(value: UnavailableKind) -> &'static str {
    match value {
        UnavailableKind::RateLimited => "rate_limited",
        UnavailableKind::ModelUnavailable => "model_unavailable",
        UnavailableKind::CircuitOpen => "circuit_open",
    }
}
