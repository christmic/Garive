use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{content, digest, enumeration, fields, non_empty, unsigned, EMPTY};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "schedule.created" => created(value),
        "schedule.claimed" => claimed(value),
        "schedule.fired" => fired(value),
        "schedule.skipped" => skipped(value),
        "schedule.cancelled" => cancelled(value),
        "schedule.failed" => failed(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn created(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "schedule_id",
            "revision_id",
            "intent",
            "intent_digest",
        ],
        EMPTY,
    )?;
    for key in ["command_id", "schedule_id", "revision_id"] {
        non_empty(value, key)?;
    }
    content(value, "intent")?;
    digest(value, "intent_digest")?;
    if value["intent_digest"] != value["intent"]["digest"] {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}

fn claimed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "schedule_id",
            "revision_id",
            "occurrence_id",
            "ordinal",
            "due_at_utc",
            "lease_id",
            "lease_epoch",
            "through_position",
        ],
        EMPTY,
    )?;
    for key in ["schedule_id", "revision_id", "occurrence_id", "lease_id"] {
        non_empty(value, key)?;
    }
    unsigned(value, "ordinal", true)?;
    timestamp(value, "due_at_utc")?;
    unsigned(value, "lease_epoch", true)?;
    unsigned(value, "through_position", false)
}

fn fired(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "schedule_id",
            "revision_id",
            "occurrence_id",
            "ordinal",
            "runtime_command_id",
            "disposition",
            "committed_position",
        ],
        EMPTY,
    )?;
    for key in [
        "schedule_id",
        "revision_id",
        "occurrence_id",
        "runtime_command_id",
    ] {
        non_empty(value, key)?;
    }
    unsigned(value, "ordinal", true)?;
    enumeration(value, "disposition", &["committed", "replayed"])?;
    unsigned(value, "committed_position", true)
}

fn skipped(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "schedule_id",
            "revision_id",
            "first_ordinal",
            "last_ordinal",
            "first_due_at_utc",
            "last_due_at_utc",
            "observed_at_utc",
        ],
        EMPTY,
    )?;
    for key in ["schedule_id", "revision_id"] {
        non_empty(value, key)?;
    }
    unsigned(value, "first_ordinal", true)?;
    unsigned(value, "last_ordinal", true)?;
    if value["first_ordinal"].as_u64() > value["last_ordinal"].as_u64() {
        return Err(LedgerError::InvalidFact);
    }
    for key in ["first_due_at_utc", "last_due_at_utc", "observed_at_utc"] {
        timestamp(value, key)?;
    }
    Ok(())
}

fn cancelled(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "schedule_id",
            "expected_revision_id",
            "reason",
        ],
        EMPTY,
    )?;
    for key in ["command_id", "schedule_id", "expected_revision_id"] {
        non_empty(value, key)?;
    }
    enumeration(
        value,
        "reason",
        &["user", "operator", "policy", "superseded"],
    )?;
    Ok(())
}

fn failed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["schedule_id", "revision_id", "reason"],
        &["occurrence_id", "ordinal"],
    )?;
    non_empty(value, "schedule_id")?;
    non_empty(value, "revision_id")?;
    let occurrence = value.contains_key("occurrence_id");
    if occurrence != value.contains_key("ordinal") {
        return Err(LedgerError::InvalidFact);
    }
    if occurrence {
        non_empty(value, "occurrence_id")?;
        unsigned(value, "ordinal", true)?;
    }
    enumeration(
        value,
        "reason",
        &[
            "invalid_schedule",
            "schedule_not_found",
            "revision_conflict",
            "subject_not_resumable",
            "authority_denied",
            "clock_invalid",
            "occurrence_overflow",
            "misfire_limit_exceeded",
            "lease_lost",
            "dispatch_conflict",
            "durability_failure",
            "corrupt_schedule_state",
        ],
    )?;
    Ok(())
}

fn timestamp(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let raw = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(LedgerError::InvalidFact)?;
    if DateTime::parse_from_rfc3339(raw).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == raw
    }) {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}
