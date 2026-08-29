use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{
    content, digest, enumeration, fields, non_empty, optional_content, optional_unsigned, unsigned,
    usage, EMPTY,
};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "model.prepared" => prepared(value),
        "model.started" => started(value),
        "model.completed" => completed(value),
        "model.rejected" => rejected(value),
        "model.interrupted" => interrupted(value),
        "model.unavailable" => unavailable(value),
        "model.uncertain" => uncertain(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn prepared(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_digest",
            "capability_target",
            "deployment_id",
            "recovery_policy_revision",
            "max_attempts",
        ],
        EMPTY,
    )?;
    digest(value, "request_digest")?;
    for key in [
        "capability_target",
        "deployment_id",
        "recovery_policy_revision",
    ] {
        non_empty(value, key)?;
    }
    unsigned(value, "max_attempts", true)
}

fn started(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["request_digest", "dispatch_attempt_id"], EMPTY)?;
    digest(value, "request_digest")?;
    non_empty(value, "dispatch_attempt_id")
}

fn completed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["request_digest", "stop_reason", "items", "usage"],
        EMPTY,
    )?;
    digest(value, "request_digest")?;
    enumeration(
        value,
        "stop_reason",
        &[
            "end_turn",
            "tool_use",
            "stop_sequence",
            "pause_turn",
            "refusal",
            "other",
        ],
    )?;
    content(value, "items")?;
    usage(value, "usage")
}

fn rejected(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["request_digest", "kind"], &["evidence"])?;
    digest(value, "request_digest")?;
    enumeration(
        value,
        "kind",
        &["context_overflow", "authentication", "content_policy"],
    )?;
    optional_content(value, "evidence")
}

fn interrupted(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["request_digest", "kind", "partial_items", "usage"],
        EMPTY,
    )?;
    digest(value, "request_digest")?;
    enumeration(value, "kind", &["cancelled", "output_limit", "transport"])?;
    content(value, "partial_items")?;
    usage(value, "usage")
}

fn unavailable(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["request_digest", "kind"], &["retry_after_ms"])?;
    digest(value, "request_digest")?;
    enumeration(
        value,
        "kind",
        &["rate_limited", "model_unavailable", "circuit_open"],
    )?;
    optional_unsigned(value, "retry_after_ms")
}

fn uncertain(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["request_digest", "reason"], &["evidence"])?;
    digest(value, "request_digest")?;
    enumeration(
        value,
        "reason",
        &["runtime_lost", "transport_lost", "provider_state_unknown"],
    )?;
    optional_content(value, "evidence")
}
