use garive_ledger::{CommitDisposition, CommitResult, FactDraft, SessionId};
use garive_memory::{
    HypothesisState, MemoryAuthority, MemoryControlDocument, MemoryDocumentLimits, MemoryKind,
    MemoryScopeClass, MemorySensitivity, MemoryType,
};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;

use crate::MemoryControlRuntimeError;

use super::{memory_control_operations as operations, storage::encode_u64};

struct FactBackedRevision {
    namespace_id: String,
    record_id: String,
    revision_id: String,
    supersedes_revision_id: Option<String>,
    document: MemoryControlDocument,
    source_index: usize,
    classification_index: usize,
}

pub(super) fn apply(
    transaction: &Transaction<'_>,
    session_id: &SessionId,
    result: &CommitResult,
    drafts: &[FactDraft],
    limits: MemoryDocumentLimits,
) -> Result<(u64, u64), MemoryControlRuntimeError> {
    let revision = decode_revision(session_id, result, drafts, limits)?;
    if result.disposition == CommitDisposition::Replayed {
        return replay(transaction, &revision, drafts);
    }
    transaction
        .execute(
            "INSERT OR IGNORE INTO memory_namespaces(namespace_id,repository_revision,source_mode) \
             VALUES (?1,?2,'fact_backed')",
            params![&revision.namespace_id, encode_u64(0)],
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let (previous, mode) = transaction
        .query_row(
            "SELECT repository_revision,source_mode FROM memory_namespaces WHERE namespace_id=?1",
            [&revision.namespace_id],
            |row| Ok((decode(row.get::<_, Vec<u8>>(0)?)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if mode != "fact_backed" {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    let committed = previous
        .checked_add(1)
        .ok_or(MemoryControlRuntimeError::StaleSnapshot)?;
    operations::insert_revision(
        transaction,
        &revision.namespace_id,
        &revision.record_id,
        &revision.revision_id,
        &revision.document,
        committed,
    )?;
    if let Some(prior) = revision.supersedes_revision_id.as_deref() {
        let current = transaction
            .query_row(
                "SELECT revision_id FROM memory_control_current WHERE namespace_id=?1 AND record_id=?2",
                params![&revision.namespace_id, &revision.record_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        if current.as_deref() != Some(prior) {
            return Err(MemoryControlRuntimeError::StaleSnapshot);
        }
        operations::replace_current(
            transaction,
            &revision.namespace_id,
            &revision.record_id,
            &revision.revision_id,
            &revision.document,
            committed,
        )?;
    } else {
        operations::insert_current(
            transaction,
            &revision.namespace_id,
            &revision.record_id,
            &revision.revision_id,
            &revision.document,
            committed,
        )?;
    }
    let source = &drafts[revision.source_index];
    let classification = &drafts[revision.classification_index];
    transaction.execute(
        "INSERT INTO memory_control_sources(namespace_id,record_id,revision_id,source_session_id,source_position,source_fact_id,source_payload_digest,classification_fact_id,classification_payload_digest,repository_revision) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![&revision.namespace_id,&revision.record_id,&revision.revision_id,session_id.as_str(),encode_u64(result.positions[revision.source_index]),source.fact_id.as_str(),source.payload.sha256(),classification.fact_id.as_str(),classification.payload.sha256(),encode_u64(committed)],
    ).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let updated = transaction.execute(
        "UPDATE memory_namespaces SET repository_revision=?1 WHERE namespace_id=?2 AND repository_revision=?3 AND source_mode='fact_backed'",
        params![encode_u64(committed),&revision.namespace_id,encode_u64(previous)],
    ).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    if updated != 1 {
        return Err(MemoryControlRuntimeError::StaleSnapshot);
    }
    Ok((previous, committed))
}

fn replay(
    transaction: &Transaction<'_>,
    revision: &FactBackedRevision,
    drafts: &[FactDraft],
) -> Result<(u64, u64), MemoryControlRuntimeError> {
    let source = &drafts[revision.source_index];
    let classification = &drafts[revision.classification_index];
    let stored = transaction.query_row(
        "SELECT source_fact_id,source_payload_digest,classification_fact_id,classification_payload_digest,repository_revision FROM memory_control_sources WHERE namespace_id=?1 AND record_id=?2 AND revision_id=?3",
        params![&revision.namespace_id,&revision.record_id,&revision.revision_id],
        |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,decode(row.get::<_,Vec<u8>>(4)?)?)),
    ).optional().map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
    let Some((source_id, source_digest, classification_id, classification_digest, committed)) =
        stored
    else {
        return Err(MemoryControlRuntimeError::PersistenceFailed);
    };
    if source_id != source.fact_id.as_str()
        || source_digest != source.payload.sha256()
        || classification_id != classification.fact_id.as_str()
        || classification_digest != classification.payload.sha256()
        || committed == 0
    {
        return Err(MemoryControlRuntimeError::PersistenceFailed);
    }
    Ok((committed - 1, committed))
}

fn decode_revision(
    session_id: &SessionId,
    result: &CommitResult,
    drafts: &[FactDraft],
    limits: MemoryDocumentLimits,
) -> Result<FactBackedRevision, MemoryControlRuntimeError> {
    if drafts.len() < 3
        || result.positions.len() != drafts.len()
        || drafts[1].kind.as_str() != "memory.committed"
        || drafts[2].kind.as_str() != "memory.revision_classified"
    {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    let committed: Value = serde_json::from_str(drafts[1].payload.as_json())
        .map_err(|_| MemoryControlRuntimeError::InvalidSnapshot)?;
    let classified: Value = serde_json::from_str(drafts[2].payload.as_json())
        .map_err(|_| MemoryControlRuntimeError::InvalidSnapshot)?;
    for key in ["namespace_id", "record_id", "revision_id"] {
        if text(&committed, key)? != text(&classified, key)? {
            return Err(MemoryControlRuntimeError::InvalidSnapshot);
        }
    }
    let source = classified
        .get("source_commit")
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)?;
    if text(source, "session_id")? != session_id.as_str()
        || number(source, "position")? != result.positions[1]
        || text(source, "fact_id")? != drafts[1].fact_id.as_str()
        || text(source, "payload_digest")? != drafts[1].payload.sha256()
    {
        return Err(MemoryControlRuntimeError::InvalidSnapshot);
    }
    let content = committed
        .get("content")
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)?;
    let inline = text(content, "inline_utf8")?;
    let document = MemoryControlDocument::from_repository_record(
        text(&committed, "record_id")?,
        text(&committed, "revision_id")?,
        authority(text(&classified, "authority")?)?,
        memory_type(text(&classified, "memory_type")?)?,
        memory_kind(text(&committed, "kind")?)?,
        scope(text(&classified, "scope")?)?,
        text(&classified, "scope_owner_id")?,
        lifecycle(text(&classified, "lifecycle")?)?,
        sensitivity(text(&committed, "sensitivity")?)?,
        inline,
        limits,
    )
    .map_err(MemoryControlRuntimeError::from)?;
    Ok(FactBackedRevision {
        namespace_id: text(&committed, "namespace_id")?.into(),
        record_id: text(&committed, "record_id")?.into(),
        revision_id: text(&committed, "revision_id")?.into(),
        supersedes_revision_id: committed
            .get("supersedes_revision_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        document,
        source_index: 1,
        classification_index: 2,
    })
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, MemoryControlRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)
}
fn number(value: &Value, key: &str) -> Result<u64, MemoryControlRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(MemoryControlRuntimeError::InvalidSnapshot)
}
fn decode(value: Vec<u8>) -> rusqlite::Result<u64> {
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(u64::from_be_bytes(bytes))
}
fn authority(value: &str) -> Result<MemoryAuthority, MemoryControlRuntimeError> {
    match value {
        "user_declared" => Ok(MemoryAuthority::UserDeclared),
        "agent_learned" => Ok(MemoryAuthority::AgentLearned),
        "organisation_published" => Ok(MemoryAuthority::OrganisationPublished),
        _ => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}
fn memory_type(value: &str) -> Result<MemoryType, MemoryControlRuntimeError> {
    match value {
        "semantic" => Ok(MemoryType::Semantic),
        "episodic" => Ok(MemoryType::Episodic),
        "lesson" => Ok(MemoryType::Lesson),
        "procedural" => Ok(MemoryType::Procedural),
        _ => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}
fn memory_kind(value: &str) -> Result<MemoryKind, MemoryControlRuntimeError> {
    match value {
        "preference" => Ok(MemoryKind::Preference),
        "constraint" => Ok(MemoryKind::Constraint),
        "decision" => Ok(MemoryKind::Decision),
        "learned_fact" => Ok(MemoryKind::LearnedFact),
        "summary" => Ok(MemoryKind::Summary),
        _ => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}
fn scope(value: &str) -> Result<MemoryScopeClass, MemoryControlRuntimeError> {
    match value {
        "session" => Ok(MemoryScopeClass::Session),
        "agent_instance" => Ok(MemoryScopeClass::AgentInstance),
        "user" => Ok(MemoryScopeClass::User),
        "project" => Ok(MemoryScopeClass::Project),
        "platform" => Ok(MemoryScopeClass::Platform),
        _ => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}
fn lifecycle(value: &str) -> Result<HypothesisState, MemoryControlRuntimeError> {
    match value {
        "candidate" => Ok(HypothesisState::Candidate),
        "active" => Ok(HypothesisState::Active),
        _ => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}
fn sensitivity(value: &str) -> Result<MemorySensitivity, MemoryControlRuntimeError> {
    match value {
        "ordinary" => Ok(MemorySensitivity::Ordinary),
        "restricted" => Ok(MemorySensitivity::Restricted),
        _ => Err(MemoryControlRuntimeError::InvalidSnapshot),
    }
}
