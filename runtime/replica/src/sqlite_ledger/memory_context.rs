use garive_memory::{
    HypothesisState, MemoryAuthority, MemoryControlDocument, MemoryDocumentLimits, MemoryRecord,
    MemoryRecordRef, MemoryScope, MemoryScopeClass, MemorySensitivity, MemoryStatus,
};
use rusqlite::Connection;

use crate::{
    core_bridge::decode_memory_record, MemoryContextRepositorySnapshot, MemoryControlAction,
    MemoryControlGrant, MemoryRepositoryError,
};

use super::{memory_control, memory_control_integrity as integrity};

pub(super) fn read(
    connection: &Connection,
    grant: &MemoryControlGrant,
    namespace_id: &str,
    limits: MemoryDocumentLimits,
    max_repository_records: usize,
    max_repository_facts: usize,
) -> Result<Option<MemoryContextRepositorySnapshot>, MemoryRepositoryError> {
    if !grant.admits_action(namespace_id, MemoryControlAction::Export) {
        return Err(MemoryRepositoryError::Unauthorized);
    }
    let mode = integrity::namespace_source_mode(connection, namespace_id)
        .map_err(|_| MemoryRepositoryError::Unavailable)?;
    match mode.as_deref() {
        None => return Ok(None),
        Some("fact_backed") => {}
        Some(_) => return Err(MemoryRepositoryError::Unavailable),
    }
    let projection = memory_control::read_projection(connection, grant, namespace_id, limits)
        .map_err(MemoryRepositoryError::from)?;
    if projection.documents.len() > max_repository_records {
        return Err(MemoryRepositoryError::BoundExceeded);
    }
    let source_facts = source_fact_count(connection, namespace_id)?;
    if source_facts > max_repository_facts {
        return Err(MemoryRepositoryError::BoundExceeded);
    }
    let mut statement = connection
        .prepare(
            "SELECT c.record_id,c.revision_id,f.payload_json \
             FROM memory_control_current c \
             JOIN memory_control_sources s ON s.namespace_id=c.namespace_id \
               AND s.record_id=c.record_id AND s.revision_id=c.revision_id \
             JOIN ledger_facts f ON f.fact_id=s.source_fact_id \
             WHERE c.namespace_id=?1 AND c.lifecycle!='erased' \
             ORDER BY c.record_id",
        )
        .map_err(|_| MemoryRepositoryError::Unavailable)?;
    let rows = statement
        .query_map([namespace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| MemoryRepositoryError::Unavailable)?;
    let mut records = Vec::new();
    for (index, row) in rows.enumerate() {
        let (record_id, revision_id, payload_json) =
            row.map_err(|_| MemoryRepositoryError::Unavailable)?;
        let document = projection
            .documents
            .get(index)
            .ok_or(MemoryRepositoryError::Corrupt)?;
        if identity(document) != Some((record_id.as_str(), revision_id.as_str())) {
            return Err(MemoryRepositoryError::Corrupt);
        }
        let record = decode_memory_record(&payload_json, MemoryStatus::Active)
            .map_err(|_| MemoryRepositoryError::Corrupt)?;
        verify_record(document, &record, namespace_id)?;
        if document.authority() == MemoryAuthority::UserDeclared
            && document.lifecycle() == HypothesisState::Active
            && document.sensitivity() == MemorySensitivity::Ordinary
        {
            records.push(record);
        }
    }
    if records.len() > max_repository_records {
        return Err(MemoryRepositoryError::BoundExceeded);
    }
    Ok(Some(MemoryContextRepositorySnapshot {
        repository_revision: projection.repository_revision,
        records,
    }))
}

fn source_fact_count(
    connection: &Connection,
    namespace_id: &str,
) -> Result<usize, MemoryRepositoryError> {
    let count: i64 = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM memory_control_sources WHERE namespace_id=?1) + \
             (SELECT COUNT(*) FROM memory_repository_transitions WHERE namespace_id=?1)",
            [namespace_id],
            |row| row.get(0),
        )
        .map_err(|_| MemoryRepositoryError::Unavailable)?;
    usize::try_from(count).map_err(|_| MemoryRepositoryError::Corrupt)
}

fn identity(document: &MemoryControlDocument) -> Option<(&str, &str)> {
    match document.record_ref() {
        MemoryRecordRef::Existing {
            record_id,
            revision_id,
        } => Some((record_id, revision_id)),
        MemoryRecordRef::New { .. } => None,
    }
}

fn verify_record(
    document: &MemoryControlDocument,
    record: &MemoryRecord,
    namespace_id: &str,
) -> Result<(), MemoryRepositoryError> {
    let normalized = record
        .content()
        .inline_utf8()
        .map(|content| format!("{}\n", content.trim_end_matches('\n')))
        .ok_or(MemoryRepositoryError::Corrupt)?;
    if record.namespace_id() != namespace_id
        || identity(document) != Some((record.record_id(), record.revision_id()))
        || document.memory_role() != record.kind()
        || document.sensitivity() != record.sensitivity()
        || document.content() != normalized
        || !scope_matches(document, record.scope())
    {
        return Err(MemoryRepositoryError::Corrupt);
    }
    Ok(())
}

fn scope_matches(document: &MemoryControlDocument, scope: &MemoryScope) -> bool {
    match scope {
        MemoryScope::Session { owner_id } => {
            document.scope() == MemoryScopeClass::Session && document.scope_owner_id() == owner_id
        }
        MemoryScope::AgentInstance { owner_id } => {
            document.scope() == MemoryScopeClass::AgentInstance
                && document.scope_owner_id() == owner_id
        }
        MemoryScope::Namespace => matches!(
            document.scope(),
            MemoryScopeClass::User | MemoryScopeClass::Project | MemoryScopeClass::Platform
        ),
    }
}
