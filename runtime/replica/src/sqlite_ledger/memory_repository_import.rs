use garive_ledger::{CommitDisposition, CommitResult, FactDraft, SessionId};
use garive_memory::MemoryImportOperation;
use rusqlite::{params, Transaction};
use serde_json::Value;

use crate::{
    MemoryControlAction, MemoryControlGrant, MemoryControlRuntimeError, MemoryImportCommand,
};

use super::{
    memory_control_integrity as integrity, memory_control_operations as operations,
    storage::encode_u64,
};

pub(super) fn apply(
    transaction: &Transaction<'_>,
    session_id: &SessionId,
    result: &CommitResult,
    drafts: &[FactDraft],
    grant: &MemoryControlGrant,
    command: &MemoryImportCommand,
) -> Result<(u64, u64), MemoryControlRuntimeError> {
    if result.disposition != CommitDisposition::Committed
        || !grant.admits_action(&command.plan().namespace_id, MemoryControlAction::Import)
    {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    let plan = command.plan();
    let previous = integrity::namespace_revision(transaction, &plan.namespace_id)?
        .ok_or(MemoryControlRuntimeError::StaleSnapshot)?;
    if previous != plan.expected_repository_revision || previous != plan.through_revision {
        return Err(MemoryControlRuntimeError::StaleSnapshot);
    }
    if integrity::namespace_source_mode(transaction, &plan.namespace_id)?.as_deref()
        != Some("fact_backed")
    {
        return Err(MemoryControlRuntimeError::ForbiddenChange);
    }
    let committed = if plan.operations.is_empty() {
        previous
    } else {
        previous
            .checked_add(1)
            .ok_or(MemoryControlRuntimeError::StaleSnapshot)?
    };
    let mut fact_index = 0usize;
    for (ordinal, operation) in plan.operations.iter().enumerate() {
        operations::apply(transaction, grant, command, operation, committed)?;
        match operation {
            MemoryImportOperation::Add {
                record_id,
                revision_id,
                ..
            } => {
                insert_source(
                    transaction,
                    session_id,
                    result,
                    drafts,
                    fact_index,
                    record_id,
                    revision_id,
                    committed,
                    ordinal,
                    command,
                    operation,
                )?;
                fact_index += 3;
            }
            MemoryImportOperation::Supersede {
                record_id,
                new_revision_id,
                ..
            } => {
                insert_source(
                    transaction,
                    session_id,
                    result,
                    drafts,
                    fact_index,
                    record_id,
                    new_revision_id,
                    committed,
                    ordinal,
                    command,
                    operation,
                )?;
                require_kind(drafts, fact_index + 3, "memory.superseded")?;
                fact_index += 4;
            }
            MemoryImportOperation::Archive {
                record_id,
                expected_active_revision_id,
                ..
            } => {
                let fact = require_kind(drafts, fact_index, "memory.lifecycle_transitioned")?;
                let value = payload(fact)?;
                if text(&value, "namespace_id")? != plan.namespace_id
                    || text(&value, "record_id")? != *record_id
                    || text(&value, "revision_id")? != *expected_active_revision_id
                    || text(&value, "from_state")? != "cold"
                    || text(&value, "to_state")? != "archived"
                    || number(&value, "last_observed_position")? != result.positions[fact_index]
                {
                    return Err(MemoryControlRuntimeError::InvalidSnapshot);
                }
                insert_transition(
                    transaction,
                    fact,
                    &plan.namespace_id,
                    record_id,
                    expected_active_revision_id,
                    "lifecycle",
                    committed,
                    ordinal,
                )?;
                fact_index += 1;
            }
            MemoryImportOperation::Erase {
                record_id,
                expected_active_revision_id,
                ..
            } => {
                let fact = require_kind(drafts, fact_index, "memory.tombstoned")?;
                let value = payload(fact)?;
                if text(&value, "namespace_id")? != plan.namespace_id
                    || text(&value, "record_id")? != *record_id
                    || text(&value, "revision_id")? != *expected_active_revision_id
                {
                    return Err(MemoryControlRuntimeError::InvalidSnapshot);
                }
                let request = require_kind(drafts, fact_index + 1, "memory.erasure_requested")?;
                verify_erasure_request(
                    request,
                    fact,
                    session_id,
                    result.positions[fact_index],
                    &plan.namespace_id,
                    record_id,
                    expected_active_revision_id,
                )?;
                insert_transition(
                    transaction,
                    fact,
                    &plan.namespace_id,
                    record_id,
                    expected_active_revision_id,
                    "tombstone",
                    committed,
                    ordinal,
                )?;
                fact_index += 2;
            }
        }
    }
    if fact_index != drafts.len() || result.positions.len() != drafts.len() {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    if committed != previous {
        let updated = transaction
            .execute(
                "UPDATE memory_namespaces SET repository_revision=?1 \
                 WHERE namespace_id=?2 AND repository_revision=?3 AND source_mode='fact_backed'",
                params![
                    encode_u64(committed),
                    &plan.namespace_id,
                    encode_u64(previous)
                ],
            )
            .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        if updated != 1 {
            return Err(MemoryControlRuntimeError::StaleSnapshot);
        }
    }
    Ok((previous, committed))
}

#[allow(clippy::too_many_arguments)]
fn insert_source(
    transaction: &Transaction<'_>,
    session_id: &SessionId,
    result: &CommitResult,
    drafts: &[FactDraft],
    index: usize,
    record_id: &str,
    revision_id: &str,
    repository_revision: u64,
    ordinal: usize,
    command: &MemoryImportCommand,
    operation: &MemoryImportOperation,
) -> Result<(), MemoryControlRuntimeError> {
    require_kind(drafts, index, "memory.proposed")?;
    let committed = require_kind(drafts, index + 1, "memory.committed")?;
    let classified = require_kind(drafts, index + 2, "memory.revision_classified")?;
    let committed_value = payload(committed)?;
    let classified_value = payload(classified)?;
    let document = command.document_for_operation(operation)?;
    let bound = document
        .bind_existing_identity(record_id, revision_id, command.max_id_bytes())
        .map_err(MemoryControlRuntimeError::from)?;
    let content = committed_value
        .get("content")
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)?;
    let source = classified_value
        .get("source_commit")
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)?;
    if text(&committed_value, "namespace_id")? != command.plan().namespace_id
        || text(&committed_value, "record_id")? != record_id
        || text(&committed_value, "revision_id")? != revision_id
        || text(content, "inline_utf8")? != bound.content()
        || text(content, "digest")? != bound.content_digest()
        || text(&classified_value, "namespace_id")? != command.plan().namespace_id
        || text(&classified_value, "record_id")? != record_id
        || text(&classified_value, "revision_id")? != revision_id
        || text(&classified_value, "authority")? != "user_declared"
        || text(&classified_value, "lifecycle")? != "active"
        || text(source, "session_id")? != session_id.as_str()
        || number(source, "position")? != result.positions[index + 1]
        || text(source, "fact_id")? != committed.fact_id.as_str()
        || text(source, "payload_digest")? != committed.payload.sha256()
    {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    transaction
        .execute(
            "INSERT INTO memory_control_sources(\
             namespace_id,record_id,revision_id,source_session_id,source_position,source_fact_id,\
             source_payload_digest,classification_fact_id,classification_payload_digest,\
             repository_revision,operation_ordinal) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                &command.plan().namespace_id,
                record_id,
                revision_id,
                session_id.as_str(),
                encode_u64(result.positions[index + 1]),
                committed.fact_id.as_str(),
                committed.payload.sha256(),
                classified.fact_id.as_str(),
                classified.payload.sha256(),
                encode_u64(repository_revision),
                encode_u64(
                    u64::try_from(ordinal)
                        .map_err(|_| MemoryControlRuntimeError::InvalidSnapshot)?
                ),
            ],
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_transition(
    transaction: &Transaction<'_>,
    fact: &FactDraft,
    namespace_id: &str,
    record_id: &str,
    revision_id: &str,
    kind: &str,
    repository_revision: u64,
    ordinal: usize,
) -> Result<(), MemoryControlRuntimeError> {
    transaction
        .execute(
            "INSERT INTO memory_repository_transitions(\
             namespace_id,record_id,revision_id,transition_kind,fact_id,payload_digest,\
             repository_revision,operation_ordinal) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                namespace_id,
                record_id,
                revision_id,
                kind,
                fact.fact_id.as_str(),
                fact.payload.sha256(),
                encode_u64(repository_revision),
                encode_u64(
                    u64::try_from(ordinal)
                        .map_err(|_| MemoryControlRuntimeError::InvalidSnapshot)?
                ),
            ],
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    Ok(())
}

fn verify_erasure_request(
    request: &FactDraft,
    tombstone: &FactDraft,
    session_id: &SessionId,
    position: u64,
    namespace_id: &str,
    record_id: &str,
    revision_id: &str,
) -> Result<(), MemoryControlRuntimeError> {
    let value = payload(request)?;
    let reference = value
        .get("tombstone_fact")
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)?;
    if text(&value, "namespace_id")? != namespace_id
        || text(&value, "record_id")? != record_id
        || text(&value, "revision_id")? != revision_id
        || text(reference, "session_id")? != session_id.as_str()
        || number(reference, "position")? != position
        || text(reference, "fact_id")? != tombstone.fact_id.as_str()
        || text(reference, "payload_digest")? != tombstone.payload.sha256()
    {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    Ok(())
}

fn require_kind<'a>(
    drafts: &'a [FactDraft],
    index: usize,
    kind: &str,
) -> Result<&'a FactDraft, MemoryControlRuntimeError> {
    drafts
        .get(index)
        .filter(|fact| fact.kind.as_str() == kind)
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)
}

fn payload(fact: &FactDraft) -> Result<Value, MemoryControlRuntimeError> {
    serde_json::from_str(fact.payload.as_json())
        .map_err(|_| MemoryControlRuntimeError::InvalidSnapshot)
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, MemoryControlRuntimeError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)
}
fn number(value: &Value, field: &str) -> Result<u64, MemoryControlRuntimeError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)
}
