use serde_json::{Map, Value};

use super::LiveHostError;

pub(super) fn effect(
    kind: &str,
    payload: &Map<String, Value>,
    current: &str,
    receipt_seen: bool,
) -> Result<(&'static str, &'static str, Option<String>), LiveHostError> {
    match kind {
        "effect.authorized" if current == "prepared" => {
            Ok(("agent.activity.authorized", "authorized", None))
        }
        "effect.denied" if matches!(current, "prepared" | "authorized") => Ok((
            "agent.activity.denied",
            "denied",
            Some(code(
                text(payload, "code")?,
                &["authorization_denied", "replacement_required"],
            )?),
        )),
        "effect.started" if matches!(current, "prepared" | "authorized") => {
            Ok(("agent.activity.started", "running", None))
        }
        "effect.completed" if current == "running" && receipt_seen => {
            Ok(("agent.activity.completed", "completed", None))
        }
        "effect.failed" if matches!(current, "authorized" | "running") => Ok((
            "agent.activity.failed",
            "failed",
            Some(code(
                text(payload, "code")?,
                &[
                    "timeout",
                    "cancelled",
                    "tool_failure",
                    "requirement_unsupported",
                    "executor_unavailable",
                ],
            )?),
        )),
        "effect.uncertain" if current == "running" => Ok((
            "agent.activity.attention_required",
            "attention_required",
            Some(code(
                text(payload, "reason")?,
                &[
                    "started_without_receipt",
                    "receipt_invalid",
                    "executor_state_unknown",
                ],
            )?),
        )),
        "effect.reconciled" if current == "attention_required" => {
            match text(payload, "decision")? {
                "completed" => Ok((
                    "agent.activity.reconciled",
                    "completed",
                    Some("reconciled_completed".to_owned()),
                )),
                "failed" => Ok((
                    "agent.activity.reconciled",
                    "failed",
                    Some("reconciled_failed".to_owned()),
                )),
                _ => Err(LiveHostError::CorruptState),
            }
        }
        _ => Err(LiveHostError::CorruptState),
    }
}

pub(super) fn code(value: &str, admitted: &[&str]) -> Result<String, LiveHostError> {
    if admitted.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(LiveHostError::CorruptState)
    }
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, LiveHostError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(LiveHostError::CorruptState)
}
