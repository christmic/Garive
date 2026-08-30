use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::Value;

use crate::{
    memory_control::{canonical_digest, hex_sha256},
    memory_export::{export_binding_digest, MemoryExportJournalEvent},
    MemoryControlAction, MemoryControlGrant, MemoryControlRuntimeError, MemoryExportCommand,
    MemoryExportReceipt, MemoryExportTarget,
};

use super::{memory_control_integrity as integrity, storage::encode_u64};

pub(super) fn commit(
    transaction: &Transaction<'_>,
    grant: &MemoryControlGrant,
    command: &MemoryExportCommand,
    target: &MemoryExportTarget,
    receipt: &MemoryExportReceipt,
) -> Result<MemoryExportReceipt, MemoryControlRuntimeError> {
    if !grant.admits_action(command.namespace_id(), MemoryControlAction::Export) {
        return Err(MemoryControlRuntimeError::Unauthorized);
    }
    let binding = export_binding_digest(command, target, &receipt.manifest_digest)?;
    if let Some(replayed) = load(transaction, command, receipt, &binding)? {
        return Ok(replayed);
    }
    let revision = integrity::namespace_revision(transaction, command.namespace_id())?
        .ok_or(MemoryControlRuntimeError::StaleSnapshot)?;
    if revision != receipt.through_repository_revision {
        return Err(MemoryControlRuntimeError::StaleSnapshot);
    }
    let sequence = integrity::next_sequence(transaction, command.namespace_id())?;
    let (receipt_json, _) = canonical_digest(receipt)?;
    let preimage = MemoryExportJournalEvent {
        schema_version: 1,
        event_id: command.event_id(),
        namespace_id: command.namespace_id(),
        command_id: command.command_id(),
        export_id: command.export_id(),
        manifest_digest: &receipt.manifest_digest,
        through_repository_revision: revision,
        receipt_digest: &receipt.receipt_digest,
        event_digest: None,
    };
    let (_, event_digest) = canonical_digest(&preimage)?;
    let event = MemoryExportJournalEvent {
        event_digest: Some(&event_digest),
        ..preimage
    };
    let (event_json, _) = canonical_digest(&event)?;
    transaction
        .execute(
            "INSERT INTO memory_control_journal(\
             namespace_id,sequence,event_id,command_id,event_kind,schema_version,binding_digest,\
             previous_repository_revision,committed_repository_revision,operations_json,\
             operations_sha256,receipt_json,receipt_sha256,event_json,event_sha256\
             ) VALUES (?1,?2,?3,?4,'export',1,?5,?6,?6,NULL,NULL,?7,?8,?9,?10)",
            params![
                command.namespace_id(),
                encode_u64(sequence),
                command.event_id(),
                command.command_id(),
                binding,
                encode_u64(revision),
                receipt_json,
                &receipt.receipt_digest,
                event_json,
                event_digest,
            ],
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    Ok(receipt.clone())
}

pub(super) fn load(
    connection: &Connection,
    command: &MemoryExportCommand,
    expected: &MemoryExportReceipt,
    binding: &str,
) -> Result<Option<MemoryExportReceipt>, MemoryControlRuntimeError> {
    let row = connection
        .query_row(
            "SELECT binding_digest,receipt_json,receipt_sha256,event_json,event_sha256 \
             FROM memory_control_journal WHERE namespace_id=?1 AND command_id=?2",
            params![command.namespace_id(), command.command_id()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let Some((stored_binding, receipt_json, receipt_digest, event_json, event_digest)) = row else {
        return Ok(None);
    };
    if stored_binding != binding {
        return Err(MemoryControlRuntimeError::CommandConflict);
    }
    let receipt = MemoryExportReceipt::decode_verified(&receipt_json)?;
    if &receipt != expected
        || receipt_digest != receipt.receipt_digest
        || !verify_event(&event_json, &event_digest, command, &receipt)
    {
        return Err(MemoryControlRuntimeError::PersistenceFailed);
    }
    Ok(Some(receipt))
}

fn verify_event(
    json: &str,
    digest: &str,
    command: &MemoryExportCommand,
    receipt: &MemoryExportReceipt,
) -> bool {
    let Ok(mut value) = serde_json::from_str::<Value>(json) else {
        return false;
    };
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    if object.get("event_id").and_then(Value::as_str) != Some(command.event_id())
        || object.get("namespace_id").and_then(Value::as_str) != Some(command.namespace_id())
        || object.get("command_id").and_then(Value::as_str) != Some(command.command_id())
        || object.get("export_id").and_then(Value::as_str) != Some(command.export_id())
        || object.get("manifest_digest").and_then(Value::as_str)
            != Some(receipt.manifest_digest.as_str())
        || object.get("receipt_digest").and_then(Value::as_str)
            != Some(receipt.receipt_digest.as_str())
        || object
            .remove("event_digest")
            .and_then(|value| value.as_str().map(str::to_owned))
            != Some(digest.to_owned())
    {
        return false;
    }
    let Ok(preimage) = serde_jcs::to_string(&value) else {
        return false;
    };
    let Ok(full) = serde_jcs::to_string(&serde_json::from_str::<Value>(json).unwrap()) else {
        return false;
    };
    hex_sha256(preimage.as_bytes()) == digest && full == json
}
