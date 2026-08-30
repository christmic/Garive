use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{digest, enumeration, fields, non_empty, unsigned, EMPTY};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    if kind != "artifact.committed" {
        return Err(LedgerError::InvalidFact);
    }
    fields(
        value,
        &[
            "artifact_id",
            "revision",
            "receipt_id",
            "display_name",
            "kind",
            "mime_type",
            "byte_size",
            "content_digest",
            "verification",
            "preview",
            "workspace_id",
            "revealable",
            "exportable",
        ],
        EMPTY,
    )?;
    for key in [
        "artifact_id",
        "receipt_id",
        "display_name",
        "mime_type",
        "workspace_id",
    ] {
        non_empty(value, key)?;
    }
    unsigned(value, "revision", true)?;
    unsigned(value, "byte_size", false)?;
    digest(value, "content_digest")?;
    enumeration(value, "kind", &["text", "file"])?;
    enumeration(
        value,
        "verification",
        &["not_run", "passed", "failed", "partial"],
    )?;
    enumeration(value, "preview", &["unavailable", "text"])?;
    for key in ["revealable", "exportable"] {
        if !value.get(key).is_some_and(Value::is_boolean) {
            return Err(LedgerError::InvalidFact);
        }
    }
    Ok(())
}
