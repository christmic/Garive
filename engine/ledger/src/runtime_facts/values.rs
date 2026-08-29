use serde_json::{Map, Value};

use crate::LedgerError;

pub(super) const EMPTY: &[&str] = &[];

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
