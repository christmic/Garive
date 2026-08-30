use garive_memory::{ContentBinding, MemoryControlDocument};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    memory_control::{canonical_digest, hex_sha256, MemoryImportJournalEvent},
    MemoryControlAction, MemoryControlGrant, MemoryControlProjection, MemoryControlRuntimeError,
    MemoryImportCommand, MemoryImportReceipt,
};

use super::storage::encode_u64;
use super::{memory_control_integrity as integrity, memory_control_operations as operations};

pub(super) fn read_projection(
    connection: &Connection,
    grant: &MemoryControlGrant,
    namespace_id: &str,
    limits: garive_memory::MemoryDocumentLimits,
) -> Result<MemoryControlProjection, MemoryControlRuntimeError> {
    if !grant.admits_action(namespace_id, MemoryControlAction::Export) {
        return Err(MemoryControlRuntimeError::Unauthorized);
    }
    let revision = integrity::namespace_revision_connection(connection, namespace_id)?
        .ok_or(MemoryControlRuntimeError::StaleSnapshot)?;
    let mut statement = connection
        .prepare(
            "SELECT record_id,revision_id,lifecycle,document_markdown,document_digest \
             FROM memory_control_current WHERE namespace_id=?1 AND lifecycle!='erased' \
             ORDER BY record_id",
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let rows = statement
        .query_map([namespace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let mut documents = Vec::new();
    for row in rows {
        let (record_id, revision_id, stored_lifecycle, markdown, digest) =
            row.map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let document = garive_memory::parse_memory_document(markdown.as_bytes(), limits)
            .map_err(MemoryControlRuntimeError::from)?;
        let (document_record, document_revision) = operations::existing_identity(&document)?;
        if document_record != record_id
            || document_revision != revision_id
            || operations::lifecycle(document.lifecycle()) != stored_lifecycle
            || document.document_digest() != digest
            || document.render() != markdown
            || !grant.admits(
                namespace_id,
                MemoryControlAction::Export,
                &operations::authorized_scope(&document),
            )
        {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
        documents.push(document);
    }
    integrity::verify_revision_content(connection, namespace_id)?;
    integrity::verify_repository_sources(connection, namespace_id)?;
    Ok(MemoryControlProjection {
        namespace_id: namespace_id.to_owned(),
        repository_revision: revision,
        documents,
    })
}

pub(super) fn initialize(
    transaction: &Transaction<'_>,
    grant: &MemoryControlGrant,
    namespace_id: &str,
    repository_revision: u64,
    documents: &[MemoryControlDocument],
) -> Result<(), MemoryControlRuntimeError> {
    if repository_revision == 0 {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    if !grant.admits_action(namespace_id, MemoryControlAction::Import) {
        return Err(MemoryControlRuntimeError::Unauthorized);
    }
    if documents
        .windows(2)
        .any(|pair| pair[0].record_ref().record_id() >= pair[1].record_ref().record_id())
        || documents.iter().any(MemoryControlDocument::erase_requested)
    {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    transaction
        .execute(
            "INSERT INTO memory_namespaces(namespace_id, repository_revision) VALUES (?1, ?2)",
            params![namespace_id, encode_u64(repository_revision)],
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let mut prior_record: Option<&str> = None;
    for document in documents {
        let (record_id, revision_id) = operations::existing_identity(document)?;
        let scope = operations::authorized_scope(document);
        if prior_record.is_some_and(|prior| prior >= record_id) {
            return Err(MemoryControlRuntimeError::InvalidSnapshot);
        }
        if !grant.admits(namespace_id, MemoryControlAction::Import, &scope) {
            return Err(MemoryControlRuntimeError::Unauthorized);
        }
        prior_record = Some(record_id);
        operations::insert_revision(
            transaction,
            namespace_id,
            record_id,
            revision_id,
            document,
            0,
        )?;
        operations::insert_current(
            transaction,
            namespace_id,
            record_id,
            revision_id,
            document,
            0,
        )?;
    }
    Ok(())
}

pub(super) fn commit_import(
    transaction: &Transaction<'_>,
    grant: &MemoryControlGrant,
    command: &MemoryImportCommand,
) -> Result<MemoryImportReceipt, MemoryControlRuntimeError> {
    command
        .plan()
        .verify()
        .map_err(MemoryControlRuntimeError::from)?;
    if !grant.admits_action(&command.plan().namespace_id, MemoryControlAction::Import) {
        return Err(MemoryControlRuntimeError::Unauthorized);
    }
    if integrity::namespace_source_mode(transaction, &command.plan().namespace_id)?.as_deref()
        == Some("fact_backed")
    {
        return Err(MemoryControlRuntimeError::ForbiddenChange);
    }
    if let Some(receipt) = replay(transaction, command)? {
        return Ok(receipt);
    }
    let plan = command.plan();
    let previous = integrity::namespace_revision(transaction, &plan.namespace_id)?
        .ok_or(MemoryControlRuntimeError::StaleSnapshot)?;
    if previous != plan.expected_repository_revision || previous != plan.through_revision {
        return Err(MemoryControlRuntimeError::StaleSnapshot);
    }
    let sequence = integrity::next_sequence(transaction, &plan.namespace_id)?;
    for operation in &plan.operations {
        operations::apply(transaction, grant, command, operation, sequence)?;
    }
    let committed = if plan.operations.is_empty() {
        previous
    } else {
        previous
            .checked_add(1)
            .ok_or(MemoryControlRuntimeError::StaleSnapshot)?
    };
    if committed != previous {
        let updated = transaction
            .execute(
                "UPDATE memory_namespaces SET repository_revision=?1 \
                 WHERE namespace_id=?2 AND repository_revision=?3",
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
    append_event(transaction, command, sequence, previous, committed)
}

fn replay(
    transaction: &Transaction<'_>,
    command: &MemoryImportCommand,
) -> Result<Option<MemoryImportReceipt>, MemoryControlRuntimeError> {
    let row = transaction
        .query_row(
            "SELECT binding_digest, operations_json, operations_sha256, receipt_json, \
             receipt_sha256, event_json, event_sha256 FROM memory_control_journal \
             WHERE namespace_id=?1 AND command_id=?2",
            params![&command.plan().namespace_id, command.command_id()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let Some((
        binding,
        operations,
        operations_digest,
        receipt_json,
        receipt_digest,
        event_json,
        event_digest,
    )) = row
    else {
        return Ok(None);
    };
    if binding != command.plan().plan_digest {
        return Err(MemoryControlRuntimeError::CommandConflict);
    }
    let expected_operations = command
        .plan()
        .canonical_operations_json()
        .map_err(MemoryControlRuntimeError::from)?;
    let receipt = MemoryImportReceipt::decode_verified(&receipt_json)?;
    if operations != expected_operations
        || operations_digest != hex_sha256(operations.as_bytes())
        || receipt_digest != receipt.receipt_digest
        || receipt.command_id != command.command_id()
        || receipt.namespace_id != command.plan().namespace_id
        || receipt.plan_digest != command.plan().plan_digest
        || !integrity::verify_event(
            &event_json,
            &event_digest,
            command,
            &operations,
            &operations_digest,
            &receipt,
        )
    {
        return Err(MemoryControlRuntimeError::PersistenceFailed);
    }
    Ok(Some(receipt))
}

fn append_event(
    transaction: &Transaction<'_>,
    command: &MemoryImportCommand,
    sequence: u64,
    previous: u64,
    committed: u64,
) -> Result<MemoryImportReceipt, MemoryControlRuntimeError> {
    let operations_json = command
        .plan()
        .canonical_operations_json()
        .map_err(MemoryControlRuntimeError::from)?;
    let operations_digest = hex_sha256(operations_json.as_bytes());
    let operations = ContentBinding::from_inline(operations_json.clone());
    let (receipt, receipt_json) = MemoryImportReceipt::create(command, previous, committed)?;
    let preimage = MemoryImportJournalEvent {
        schema_version: 1,
        event_id: command.event_id(),
        namespace_id: &command.plan().namespace_id,
        command_id: command.command_id(),
        plan_digest: &command.plan().plan_digest,
        previous_repository_revision: previous,
        committed_repository_revision: committed,
        operations: &operations,
        receipt_digest: &receipt.receipt_digest,
        event_digest: None,
    };
    let (_, event_digest) = canonical_digest(&preimage)?;
    let event = MemoryImportJournalEvent {
        event_digest: Some(&event_digest),
        ..preimage
    };
    let (event_json, _) = canonical_digest(&event)?;
    transaction
        .execute(
            "INSERT INTO memory_control_journal(\
             namespace_id, sequence, event_id, command_id, event_kind, schema_version, \
             binding_digest, previous_repository_revision, committed_repository_revision, \
             operations_json, operations_sha256, receipt_json, receipt_sha256, event_json, event_sha256\
             ) VALUES (?1,?2,?3,?4,'import',1,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                &command.plan().namespace_id,
                encode_u64(sequence),
                command.event_id(),
                command.command_id(),
                &command.plan().plan_digest,
                encode_u64(previous),
                encode_u64(committed),
                operations_json,
                operations_digest,
                receipt_json,
                &receipt.receipt_digest,
                event_json,
                event_digest,
            ],
        )
        .map_err(|error| {
            if error.to_string().contains("command_id") {
                MemoryControlRuntimeError::CommandConflict
            } else {
                MemoryControlRuntimeError::PersistenceFailed
            }
        })?;
    Ok(receipt)
}
