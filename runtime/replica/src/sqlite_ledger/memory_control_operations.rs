use garive_memory::{
    HypothesisState, MemoryAuthorizedScope, MemoryControlDocument, MemoryImportOperation,
    MemoryRecordRef,
};
use rusqlite::{params, OptionalExtension, Transaction};

use crate::{
    MemoryControlAction, MemoryControlGrant, MemoryControlRuntimeError, MemoryImportCommand,
};

use super::storage::encode_u64;

pub(super) fn apply(
    transaction: &Transaction<'_>,
    grant: &MemoryControlGrant,
    command: &MemoryImportCommand,
    operation: &MemoryImportOperation,
    sequence: u64,
) -> Result<(), MemoryControlRuntimeError> {
    let document = matching_document(command, operation)?;
    if !grant.admits(
        &command.plan().namespace_id,
        MemoryControlAction::Import,
        &authorized_scope(document),
    ) {
        return Err(MemoryControlRuntimeError::Unauthorized);
    }
    match operation {
        MemoryImportOperation::Add {
            record_id,
            revision_id,
            ..
        } => {
            require_absent(transaction, &command.plan().namespace_id, record_id)?;
            let bound = document
                .bind_existing_identity(record_id, revision_id, command.max_id_bytes())
                .map_err(MemoryControlRuntimeError::from)?;
            insert_revision(
                transaction,
                &command.plan().namespace_id,
                record_id,
                revision_id,
                &bound,
                sequence,
            )?;
            insert_current(
                transaction,
                &command.plan().namespace_id,
                record_id,
                revision_id,
                &bound,
                sequence,
            )
        }
        MemoryImportOperation::Supersede {
            record_id,
            expected_active_revision_id,
            new_revision_id,
            ..
        } => {
            require_current(
                transaction,
                &command.plan().namespace_id,
                record_id,
                expected_active_revision_id,
            )?;
            let bound = document
                .bind_existing_identity(record_id, new_revision_id, command.max_id_bytes())
                .map_err(MemoryControlRuntimeError::from)?;
            insert_revision(
                transaction,
                &command.plan().namespace_id,
                record_id,
                new_revision_id,
                &bound,
                sequence,
            )?;
            replace_current(
                transaction,
                &command.plan().namespace_id,
                record_id,
                new_revision_id,
                &bound,
                sequence,
            )
        }
        MemoryImportOperation::Archive {
            record_id,
            expected_active_revision_id,
            ..
        } => {
            require_current(
                transaction,
                &command.plan().namespace_id,
                record_id,
                expected_active_revision_id,
            )?;
            replace_current(
                transaction,
                &command.plan().namespace_id,
                record_id,
                expected_active_revision_id,
                document,
                sequence,
            )
        }
        MemoryImportOperation::Erase {
            record_id,
            expected_active_revision_id,
            ..
        } => {
            require_current(
                transaction,
                &command.plan().namespace_id,
                record_id,
                expected_active_revision_id,
            )?;
            erase_record(
                transaction,
                &command.plan().namespace_id,
                record_id,
                sequence,
            )
        }
    }
}

fn matching_document<'a>(
    command: &'a MemoryImportCommand,
    operation: &MemoryImportOperation,
) -> Result<&'a MemoryControlDocument, MemoryControlRuntimeError> {
    let matches = command
        .documents()
        .iter()
        .filter(|document| {
            let identity_matches = match (operation, document.record_ref()) {
                (
                    MemoryImportOperation::Add {
                        source_draft_token, ..
                    },
                    MemoryRecordRef::New { draft_token },
                ) => source_draft_token == draft_token,
                (
                    MemoryImportOperation::Supersede {
                        record_id,
                        expected_active_revision_id,
                        authority,
                        ..
                    },
                    MemoryRecordRef::Existing {
                        record_id: document_record,
                        revision_id,
                    },
                ) => {
                    record_id == document_record
                        && expected_active_revision_id == revision_id
                        && *authority == document.authority()
                        && !document.erase_requested()
                }
                (
                    MemoryImportOperation::Archive {
                        record_id,
                        expected_active_revision_id,
                        ..
                    },
                    MemoryRecordRef::Existing {
                        record_id: document_record,
                        revision_id,
                    },
                ) => {
                    record_id == document_record
                        && expected_active_revision_id == revision_id
                        && document.lifecycle() == HypothesisState::Archived
                        && !document.erase_requested()
                }
                (
                    MemoryImportOperation::Erase {
                        record_id,
                        expected_active_revision_id,
                        ..
                    },
                    MemoryRecordRef::Existing {
                        record_id: document_record,
                        revision_id,
                    },
                ) => {
                    record_id == document_record
                        && expected_active_revision_id == revision_id
                        && document.erase_requested()
                }
                _ => false,
            };
            identity_matches && operation_document_digest(operation) == document.document_digest()
        })
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        Ok(matches[0])
    } else {
        Err(MemoryControlRuntimeError::InvalidSnapshot)
    }
}

fn operation_document_digest(operation: &MemoryImportOperation) -> &str {
    match operation {
        MemoryImportOperation::Add {
            document_digest, ..
        }
        | MemoryImportOperation::Supersede {
            document_digest, ..
        }
        | MemoryImportOperation::Archive {
            document_digest, ..
        }
        | MemoryImportOperation::Erase {
            document_digest, ..
        } => document_digest,
    }
}

pub(super) fn authorized_scope(document: &MemoryControlDocument) -> MemoryAuthorizedScope {
    MemoryAuthorizedScope {
        scope: document.scope(),
        owner_id: document.scope_owner_id().to_owned(),
    }
}

pub(super) fn existing_identity(
    document: &MemoryControlDocument,
) -> Result<(&str, &str), MemoryControlRuntimeError> {
    match document.record_ref() {
        MemoryRecordRef::Existing {
            record_id,
            revision_id,
        } => Ok((record_id, revision_id)),
        MemoryRecordRef::New { .. } => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}

pub(super) fn lifecycle(value: HypothesisState) -> &'static str {
    match value {
        HypothesisState::Candidate => "candidate",
        HypothesisState::Active => "active",
        HypothesisState::Cold => "cold",
        HypothesisState::Archived => "archived",
        HypothesisState::Promoted => "promoted",
    }
}

fn require_absent(
    transaction: &Transaction<'_>,
    namespace: &str,
    record: &str,
) -> Result<(), MemoryControlRuntimeError> {
    let count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM memory_control_current WHERE namespace_id=?1 AND record_id=?2",
            params![namespace, record],
            |row| row.get(0),
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if count == 0 {
        Ok(())
    } else {
        Err(MemoryControlRuntimeError::StaleSnapshot)
    }
}

fn require_current(
    transaction: &Transaction<'_>,
    namespace: &str,
    record: &str,
    revision: &str,
) -> Result<(), MemoryControlRuntimeError> {
    let current = transaction
        .query_row(
            "SELECT revision_id FROM memory_control_current WHERE namespace_id=?1 AND record_id=?2",
            params![namespace, record],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if current.as_deref() == Some(revision) {
        Ok(())
    } else {
        Err(MemoryControlRuntimeError::StaleSnapshot)
    }
}

pub(super) fn insert_revision(
    transaction: &Transaction<'_>,
    namespace: &str,
    record: &str,
    revision: &str,
    document: &MemoryControlDocument,
    sequence: u64,
) -> Result<(), MemoryControlRuntimeError> {
    transaction.execute("INSERT INTO memory_control_revisions(namespace_id,record_id,revision_id,document_markdown,document_digest,created_sequence,erased_sequence) VALUES (?1,?2,?3,?4,?5,?6,NULL)", params![namespace,record,revision,document.render(),document.document_digest(),encode_u64(sequence)]).map(|_| ()).map_err(|_| MemoryControlRuntimeError::StaleSnapshot)
}

pub(super) fn insert_current(
    transaction: &Transaction<'_>,
    namespace: &str,
    record: &str,
    revision: &str,
    document: &MemoryControlDocument,
    sequence: u64,
) -> Result<(), MemoryControlRuntimeError> {
    transaction.execute("INSERT INTO memory_control_current(namespace_id,record_id,revision_id,lifecycle,document_markdown,document_digest,updated_sequence) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![namespace,record,revision,lifecycle(document.lifecycle()),document.render(),document.document_digest(),encode_u64(sequence)]).map(|_| ()).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)
}

pub(super) fn replace_current(
    transaction: &Transaction<'_>,
    namespace: &str,
    record: &str,
    revision: &str,
    document: &MemoryControlDocument,
    sequence: u64,
) -> Result<(), MemoryControlRuntimeError> {
    let updated = transaction.execute("UPDATE memory_control_current SET revision_id=?1,lifecycle=?2,document_markdown=?3,document_digest=?4,updated_sequence=?5 WHERE namespace_id=?6 AND record_id=?7", params![revision,lifecycle(document.lifecycle()),document.render(),document.document_digest(),encode_u64(sequence),namespace,record]).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if updated == 1 {
        Ok(())
    } else {
        Err(MemoryControlRuntimeError::StaleSnapshot)
    }
}

pub(super) fn erase_record(
    transaction: &Transaction<'_>,
    namespace: &str,
    record: &str,
    sequence: u64,
) -> Result<(), MemoryControlRuntimeError> {
    transaction.execute("UPDATE memory_control_revisions SET document_markdown=NULL,erased_sequence=?1 WHERE namespace_id=?2 AND record_id=?3", params![encode_u64(sequence),namespace,record]).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let updated = transaction.execute("UPDATE memory_control_current SET lifecycle='erased',document_markdown=NULL,document_digest=NULL,updated_sequence=?1 WHERE namespace_id=?2 AND record_id=?3", params![encode_u64(sequence),namespace,record]).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if updated == 1 {
        Ok(())
    } else {
        Err(MemoryControlRuntimeError::StaleSnapshot)
    }
}
