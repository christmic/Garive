use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{enumeration, fields, non_empty, unsigned, EMPTY};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "workspace.attached" => attached(value),
        _ => Err(LedgerError::InvalidFact),
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
    enumeration(value, "access", &["enumerate"])?;
    Ok(())
}
