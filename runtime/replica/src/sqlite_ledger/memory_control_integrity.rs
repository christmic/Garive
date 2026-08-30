use rusqlite::{Connection, OptionalExtension, Transaction};
use serde_json::Value;

use crate::{
    memory_control::hex_sha256, MemoryControlRuntimeError, MemoryImportCommand, MemoryImportReceipt,
};

pub(super) fn verify_event(
    json: &str,
    digest: &str,
    command: &MemoryImportCommand,
    operations: &str,
    operations_digest: &str,
    receipt: &MemoryImportReceipt,
) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(binding) = object.get("operations").and_then(Value::as_object) else {
        return false;
    };
    verify_embedded_digest(json, "event_digest", digest)
        && object.get("schema_version").and_then(Value::as_u64) == Some(1)
        && object.get("namespace_id").and_then(Value::as_str)
            == Some(command.plan().namespace_id.as_str())
        && object.get("command_id").and_then(Value::as_str) == Some(command.command_id())
        && object.get("plan_digest").and_then(Value::as_str)
            == Some(command.plan().plan_digest.as_str())
        && object.get("receipt_digest").and_then(Value::as_str)
            == Some(receipt.receipt_digest.as_str())
        && binding.get("inline_utf8").and_then(Value::as_str) == Some(operations)
        && binding.get("digest").and_then(Value::as_str) == Some(operations_digest)
        && binding.get("reference").is_none()
}

pub(super) fn namespace_revision(
    transaction: &Transaction<'_>,
    namespace_id: &str,
) -> Result<Option<u64>, MemoryControlRuntimeError> {
    namespace_revision_inner(transaction, namespace_id)
}

pub(super) fn namespace_revision_connection(
    connection: &Connection,
    namespace_id: &str,
) -> Result<Option<u64>, MemoryControlRuntimeError> {
    namespace_revision_inner(connection, namespace_id)
}

pub(super) fn verify_revision_content(
    connection: &Connection,
    namespace_id: &str,
) -> Result<(), MemoryControlRuntimeError> {
    let mut statement = connection
        .prepare(
            "SELECT document_markdown,document_digest FROM memory_control_revisions \
             WHERE namespace_id=?1 AND document_markdown IS NOT NULL",
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let rows = statement
        .query_map([namespace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    for row in rows {
        let (markdown, digest) = row.map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        if hex_sha256(markdown.as_bytes()) != digest {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
    }
    Ok(())
}

pub(super) fn next_sequence(
    transaction: &Transaction<'_>,
    namespace_id: &str,
) -> Result<u64, MemoryControlRuntimeError> {
    let value: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT MAX(sequence) FROM memory_control_journal WHERE namespace_id=?1",
            [namespace_id],
            |row| row.get(0),
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    value.map_or(Ok(1), |bytes| {
        decode_u64(&bytes)?
            .checked_add(1)
            .ok_or(MemoryControlRuntimeError::PersistenceFailed)
    })
}

fn verify_embedded_digest(json: &str, field: &str, expected: &str) -> bool {
    let Ok(mut value) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    if object
        .remove(field)
        .and_then(|value| value.as_str().map(str::to_owned))
        .as_deref()
        != Some(expected)
    {
        return false;
    }
    serde_jcs::to_string(&value).is_ok_and(|canonical| hex_sha256(canonical.as_bytes()) == expected)
        && serde_jcs::to_string(&serde_json::from_str::<Value>(json).unwrap())
            .is_ok_and(|canonical| canonical == json)
}

fn namespace_revision_inner(
    connection: &Connection,
    namespace_id: &str,
) -> Result<Option<u64>, MemoryControlRuntimeError> {
    let bytes = connection
        .query_row(
            "SELECT repository_revision FROM memory_namespaces WHERE namespace_id=?1",
            [namespace_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    bytes.map(|value| decode_u64(&value)).transpose()
}

fn decode_u64(value: &[u8]) -> Result<u64, MemoryControlRuntimeError> {
    value
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)
}
