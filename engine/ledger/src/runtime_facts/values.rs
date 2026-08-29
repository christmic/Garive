use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::LedgerError;

pub(super) const EMPTY: &[&str] = &[];
const DIGEST_LENGTH: usize = 64;

pub(super) fn object(value: &Value) -> Result<&Map<String, Value>, LedgerError> {
    value.as_object().ok_or(LedgerError::InvalidFact)
}

pub(super) fn fields(
    value: &Map<String, Value>,
    required: &[&str],
    optional: &[&str],
) -> Result<(), LedgerError> {
    if required.iter().any(|key| !value.contains_key(*key))
        || value
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        Err(LedgerError::InvalidFact)
    } else {
        Ok(())
    }
}

pub(super) fn string<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, LedgerError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(LedgerError::InvalidFact)
}

pub(super) fn non_empty(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    if string(value, key)?.is_empty() {
        Err(LedgerError::InvalidFact)
    } else {
        Ok(())
    }
}

pub(super) fn enumeration<'a>(
    value: &'a Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<&'a str, LedgerError> {
    let found = string(value, key)?;
    if allowed.contains(&found) {
        Ok(found)
    } else {
        Err(LedgerError::InvalidFact)
    }
}

pub(super) fn unsigned(
    value: &Map<String, Value>,
    key: &str,
    nonzero: bool,
) -> Result<(), LedgerError> {
    let found = value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(LedgerError::InvalidFact)?;
    if nonzero && found == 0 {
        Err(LedgerError::InvalidFact)
    } else {
        Ok(())
    }
}

pub(super) fn digest(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let found = string(value, key)?;
    if found.len() == DIGEST_LENGTH
        && found
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}

pub(super) fn content(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let binding = object(value.get(key).ok_or(LedgerError::InvalidFact)?)?;
    fields(binding, &["digest"], &["inline_utf8", "reference"])?;
    digest(binding, "digest")?;
    match (binding.get("inline_utf8"), binding.get("reference")) {
        (Some(Value::String(text)), None) => {
            let actual = format!("{:x}", Sha256::digest(text.as_bytes()));
            if actual == string(binding, "digest")? {
                Ok(())
            } else {
                Err(LedgerError::InvalidFact)
            }
        }
        (None, Some(Value::String(reference))) if !reference.is_empty() => Ok(()),
        _ => Err(LedgerError::InvalidFact),
    }
}

pub(super) fn optional_content(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    if value.contains_key(key) {
        content(value, key)
    } else {
        Ok(())
    }
}

pub(super) fn usage(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let usage = object(value.get(key).ok_or(LedgerError::InvalidFact)?)?;
    fields(
        usage,
        &["input_tokens", "output_tokens", "source"],
        &["cache_read_tokens", "cache_write_tokens"],
    )?;
    for name in [
        "input_tokens",
        "output_tokens",
        "cache_read_tokens",
        "cache_write_tokens",
    ] {
        if let Some(count) = usage.get(name) {
            token_count(object(count)?)?;
        }
    }
    enumeration(usage, "source", &["provider_reported", "estimated"])?;
    Ok(())
}

fn token_count(value: &Map<String, Value>) -> Result<(), LedgerError> {
    match value.get("kind").and_then(Value::as_str) {
        Some("unknown") => fields(value, &["kind"], EMPTY),
        Some("known") => {
            fields(value, &["kind", "value"], EMPTY)?;
            unsigned(value, "value", false)
        }
        _ => Err(LedgerError::InvalidFact),
    }
}

pub(super) fn limits(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let limits = object(value.get(key).ok_or(LedgerError::InvalidFact)?)?;
    fields(
        limits,
        &["max_iterations"],
        &[
            "max_input_tokens",
            "max_output_tokens",
            "deadline_budget_ms",
        ],
    )?;
    for name in [
        "max_iterations",
        "max_input_tokens",
        "max_output_tokens",
        "deadline_budget_ms",
    ] {
        if limits.contains_key(name) {
            unsigned(limits, name, true)?;
        }
    }
    Ok(())
}
