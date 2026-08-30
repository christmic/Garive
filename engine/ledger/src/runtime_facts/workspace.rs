use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{content, enumeration, fields, non_empty, object, string, unsigned, EMPTY};

const MAX_CONTEXT_ENTRIES: usize = 8;
const MAX_CONTEXT_BYTES: usize = 60 * 1_024;
const MAX_PUBLIC_TEXT_BYTES: usize = 128;

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "workspace.attached" => attached(value),
        "workspace.context_selected" => context_selected(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn context_selected(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["command_id", "workspace_id", "grant_revision", "entries"],
        EMPTY,
    )?;
    bounded_text(value, "command_id")?;
    bounded_text(value, "workspace_id")?;
    unsigned(value, "grant_revision", true)?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_CONTEXT_ENTRIES)
        .ok_or(LedgerError::InvalidFact)?;
    let mut identities = BTreeSet::new();
    let mut total_bytes = 0usize;
    for entry in entries {
        let entry = object(entry)?;
        fields(
            entry,
            &["entry_id", "display_name", "kind", "content"],
            EMPTY,
        )?;
        bounded_text(entry, "entry_id")?;
        bounded_text(entry, "display_name")?;
        enumeration(entry, "kind", &["text"])?;
        content(entry, "content")?;
        if !identities.insert(string(entry, "entry_id")?) {
            return Err(LedgerError::InvalidFact);
        }
        let body = object(entry.get("content").ok_or(LedgerError::InvalidFact)?)?;
        total_bytes = total_bytes
            .checked_add(string(body, "inline_utf8")?.len())
            .filter(|total| *total <= MAX_CONTEXT_BYTES)
            .ok_or(LedgerError::InvalidFact)?;
    }
    Ok(())
}

fn bounded_text(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    non_empty(value, key)?;
    if string(value, key)?.len() > MAX_PUBLIC_TEXT_BYTES {
        Err(LedgerError::InvalidFact)
    } else {
        Ok(())
    }
}

fn attached(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "workspace_id",
            "display_name",
            "grant_revision",
            "access",
        ],
        EMPTY,
    )?;
    non_empty(value, "command_id")?;
    non_empty(value, "workspace_id")?;
    non_empty(value, "display_name")?;
    unsigned(value, "grant_revision", true)?;
    enumeration(value, "access", &["enumerate", "read_write"])?;
    Ok(())
}
