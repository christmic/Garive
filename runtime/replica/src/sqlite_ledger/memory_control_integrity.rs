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

pub(super) fn namespace_source_mode(
    connection: &Connection,
    namespace_id: &str,
) -> Result<Option<String>, MemoryControlRuntimeError> {
    connection
        .query_row(
            "SELECT source_mode FROM memory_namespaces WHERE namespace_id=?1",
            [namespace_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)
}

pub(super) fn verify_repository_sources(
    connection: &Connection,
    namespace_id: &str,
) -> Result<(), MemoryControlRuntimeError> {
    let mode = namespace_source_mode(connection, namespace_id)?
        .ok_or(MemoryControlRuntimeError::PersistenceFailed)?;
    let source_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM memory_control_sources WHERE namespace_id=?1",
            [namespace_id],
            |row| row.get(0),
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if mode == "isolated" {
        return if source_count == 0 {
            Ok(())
        } else {
            Err(MemoryControlRuntimeError::PersistenceFailed)
        };
    }
    let revision_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM memory_control_revisions WHERE namespace_id=?1",
            [namespace_id],
            |row| row.get(0),
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if source_count != revision_count {
        return Err(MemoryControlRuntimeError::PersistenceFailed);
    }
    let mut statement = connection
        .prepare(
            "SELECT s.record_id,s.revision_id,s.source_session_id,s.source_position,\
         s.source_fact_id,s.source_payload_digest,s.classification_fact_id,\
         s.classification_payload_digest,f.session_id,f.position,f.kind,f.payload_sha256,\
         c.session_id,c.kind,c.payload_json,c.payload_sha256 \
         FROM memory_control_sources s \
         JOIN ledger_facts f ON f.fact_id=s.source_fact_id \
         JOIN ledger_facts c ON c.fact_id=s.classification_fact_id \
         WHERE s.namespace_id=?1 ORDER BY s.record_id,s.revision_id",
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let rows = statement
        .query_map([namespace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Vec<u8>>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
            ))
        })
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    for row in rows {
        let (
            record,
            revision,
            source_session,
            source_position,
            source_id,
            source_digest,
            classification_id,
            classification_digest,
            fact_session,
            fact_position,
            fact_kind,
            fact_digest,
            classification_session,
            classification_kind,
            classification_json,
            stored_classification_digest,
        ) = row.map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let classification: Value = serde_json::from_str(&classification_json)
            .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let reference = classification
            .get("source_commit")
            .and_then(Value::as_object)
            .ok_or(MemoryControlRuntimeError::PersistenceFailed)?;
        if source_session != fact_session
            || source_position != fact_position
            || source_id
                != reference
                    .get("fact_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            || source_session
                != reference
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            || decode_u64(&source_position)?
                != reference
                    .get("position")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            || source_digest != fact_digest
            || source_digest
                != reference
                    .get("payload_digest")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            || fact_kind != "memory.committed"
            || classification_kind != "memory.revision_classified"
            || classification_session != source_session
            || classification_digest != stored_classification_digest
            || classification.get("record_id").and_then(Value::as_str) != Some(record.as_str())
            || classification.get("revision_id").and_then(Value::as_str) != Some(revision.as_str())
            || classification_id.is_empty()
        {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
    }
    let (distinct, minimum, maximum): (i64, Option<Vec<u8>>, Option<Vec<u8>>) =
        connection
            .query_row(
                "SELECT COUNT(DISTINCT repository_revision),MIN(repository_revision),MAX(repository_revision) FROM (\
                 SELECT repository_revision FROM memory_control_sources WHERE namespace_id=?1 \
                 UNION ALL SELECT repository_revision FROM memory_repository_transitions WHERE namespace_id=?1)",
                [namespace_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let revision = namespace_revision_connection(connection, namespace_id)?
        .ok_or(MemoryControlRuntimeError::PersistenceFailed)?;
    if u64::try_from(distinct).ok() != Some(revision)
        || minimum.as_deref().map(decode_u64).transpose()? != Some(1)
        || maximum.as_deref().map(decode_u64).transpose()? != Some(revision)
    {
        return Err(MemoryControlRuntimeError::PersistenceFailed);
    }
    let mut operation_sets = connection
        .prepare(
            "SELECT repository_revision,COUNT(*),COUNT(DISTINCT operation_ordinal),\
             MIN(operation_ordinal),MAX(operation_ordinal) FROM (\
             SELECT repository_revision,operation_ordinal FROM memory_control_sources WHERE namespace_id=?1 \
             UNION ALL SELECT repository_revision,operation_ordinal FROM memory_repository_transitions WHERE namespace_id=?1) \
             GROUP BY repository_revision ORDER BY repository_revision",
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let rows = operation_sets
        .query_map([namespace_id], |row| {
            Ok((
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    for row in rows {
        let (count, distinct_ordinals, minimum, maximum) =
            row.map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let count =
            u64::try_from(count).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        if count == 0
            || u64::try_from(distinct_ordinals).ok() != Some(count)
            || decode_u64(&minimum)? != 0
            || decode_u64(&maximum)?.checked_add(1) != Some(count)
        {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
    }
    let mut transitions = connection.prepare(
        "SELECT t.record_id,t.revision_id,t.transition_kind,t.payload_digest,f.kind,f.payload_json,f.payload_sha256 \
         FROM memory_repository_transitions t JOIN ledger_facts f ON f.fact_id=t.fact_id \
         WHERE t.namespace_id=?1 ORDER BY t.repository_revision,t.operation_ordinal",
    ).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let rows = transitions
        .query_map([namespace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    for row in rows {
        let (record, revision, transition, digest, fact_kind, payload_json, fact_digest) =
            row.map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let payload: Value = serde_json::from_str(&payload_json)
            .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let expected_kind = match transition.as_str() {
            "tombstone" => "memory.tombstoned",
            "lifecycle" => "memory.lifecycle_transitioned",
            _ => return Err(MemoryControlRuntimeError::PersistenceFailed),
        };
        if fact_kind != expected_kind
            || digest != fact_digest
            || payload.get("namespace_id").and_then(Value::as_str) != Some(namespace_id)
            || payload.get("record_id").and_then(Value::as_str) != Some(record.as_str())
            || payload.get("revision_id").and_then(Value::as_str) != Some(revision.as_str())
        {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
    }
    Ok(())
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
