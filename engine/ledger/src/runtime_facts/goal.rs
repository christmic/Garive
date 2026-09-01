use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{
    content, digest, fields, non_empty, optional_content, optional_non_empty, unsigned, EMPTY,
};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "goal.created" => created(value),
        "goal.revised" => revised(value),
        "goal.activated" => activated(value),
        "goal.suspended" => suspended(value),
        "goal.succeeded" => succeeded(value),
        "goal.failed" => failed(value),
        "goal.cancelled" => cancelled(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn created(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "revision",
            "definition_digest",
            "definition",
            "actor_reference",
        ],
        EMPTY,
    )?;
    common(value)?;
    if value.get("revision").and_then(Value::as_u64) != Some(1) {
        return Err(LedgerError::InvalidFact);
    }
    digest(value, "definition_digest")?;
    content(value, "definition")?;
    non_empty(value, "actor_reference")
}

fn revised(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "previous_revision",
            "revision",
            "previous_definition_digest",
            "definition_digest",
            "definition",
            "replacement_reason",
            "actor_reference",
        ],
        EMPTY,
    )?;
    common(value)?;
    unsigned(value, "previous_revision", true)?;
    if value
        .get("previous_revision")
        .and_then(Value::as_u64)
        .and_then(|item| item.checked_add(1))
        != value.get("revision").and_then(Value::as_u64)
    {
        return Err(LedgerError::InvalidFact);
    }
    digest(value, "previous_definition_digest")?;
    digest(value, "definition_digest")?;
    content(value, "definition")?;
    non_empty(value, "replacement_reason")?;
    non_empty(value, "actor_reference")
}

fn activated(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "revision",
            "attempt_number",
            "actor_reference",
        ],
        &["plan_reference"],
    )?;
    common(value)?;
    unsigned(value, "attempt_number", true)?;
    optional_non_empty(value, "plan_reference")?;
    non_empty(value, "actor_reference")
}

fn suspended(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "revision",
            "reason",
            "actor_reference",
        ],
        &["suspension_reference"],
    )?;
    common(value)?;
    non_empty(value, "reason")?;
    optional_non_empty(value, "suspension_reference")?;
    non_empty(value, "actor_reference")
}

fn succeeded(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "revision",
            "evidence",
            "actor_reference",
        ],
        EMPTY,
    )?;
    common(value)?;
    content(value, "evidence")?;
    non_empty(value, "actor_reference")
}

fn failed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "revision",
            "code",
            "actor_reference",
        ],
        &["evidence"],
    )?;
    common(value)?;
    non_empty(value, "code")?;
    optional_content(value, "evidence")?;
    non_empty(value, "actor_reference")
}

fn cancelled(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "goal_id",
            "revision",
            "reason",
            "actor_reference",
        ],
        EMPTY,
    )?;
    common(value)?;
    non_empty(value, "reason")?;
    non_empty(value, "actor_reference")
}

fn common(value: &Map<String, Value>) -> Result<(), LedgerError> {
    non_empty(value, "command_id")?;
    non_empty(value, "goal_id")?;
    unsigned(value, "revision", true)
}
