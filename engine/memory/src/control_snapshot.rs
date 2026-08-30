//! Canonical M2 snapshot projection and package validation.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::{
    control_plane::hex_sha256, parse_memory_document, HypothesisState, MemoryAuthority,
    MemoryControlDocument, MemoryControlError, MemoryKind, MemoryRecordRef, MemoryScopeClass,
    MemorySensitivity, MemorySnapshot, MemorySnapshotEntry, MemorySnapshotFile,
    MemorySnapshotLimits, MemorySnapshotManifest, MemoryType,
};

#[derive(Serialize)]
struct ManifestPreimage<'a> {
    schema_version: u8,
    export_id: &'a str,
    namespace_id: &'a str,
    through_revision: u64,
    exported_at: &'a str,
    entries: &'a [MemorySnapshotEntry],
}

/// Projects current documents into one canonical M2 manifest and package.
pub fn project_memory_snapshot(
    export_id: &str,
    namespace_id: &str,
    through_revision: u64,
    exported_at: &str,
    documents: Vec<MemoryControlDocument>,
) -> Result<MemorySnapshot, MemoryControlError> {
    validate_header(export_id, namespace_id, through_revision, exported_at)?;
    let mut pairs = Vec::with_capacity(documents.len());
    for document in documents {
        let MemoryRecordRef::Existing {
            record_id,
            revision_id,
        } = document.record_ref()
        else {
            return Err(MemoryControlError::InvalidSnapshot);
        };
        if document.erase_requested() {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        let file_name = expected_file_name(record_id);
        pairs.push((
            MemorySnapshotEntry {
                record_id: record_id.clone(),
                revision_id: revision_id.clone(),
                file_name: file_name.clone(),
                authority: authority_name(document.authority()).into(),
                memory_type: type_name(document.memory_type()).into(),
                memory_role: role_name(document.memory_role()).into(),
                scope: scope_name(document.scope()).into(),
                scope_owner_id: document.scope_owner_id().into(),
                lifecycle: lifecycle_name(document.lifecycle()).into(),
                sensitivity: sensitivity_name(document.sensitivity()).into(),
                content_digest: document.content_digest(),
                document_digest: document.document_digest(),
            },
            document,
        ));
    }
    pairs.sort_by(|(left, _), (right, _)| {
        left.record_id
            .cmp(&right.record_id)
            .then(left.revision_id.cmp(&right.revision_id))
    });
    if pairs
        .windows(2)
        .any(|pair| pair[0].0.record_id == pair[1].0.record_id)
    {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let entries: Vec<_> = pairs.iter().map(|(entry, _)| entry.clone()).collect();
    validate_unique_entries(&entries)?;
    let preimage = ManifestPreimage {
        schema_version: 1,
        export_id,
        namespace_id,
        through_revision,
        exported_at,
        entries: &entries,
    };
    let bytes = serde_jcs::to_vec(&preimage).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    let manifest = MemorySnapshotManifest {
        schema_version: 1,
        export_id: export_id.into(),
        namespace_id: namespace_id.into(),
        through_revision,
        exported_at: exported_at.into(),
        entries,
        manifest_digest: hex_sha256(&bytes),
    };
    let manifest_json =
        serde_jcs::to_vec(&manifest).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    let documents = pairs
        .into_iter()
        .map(|(entry, document)| (entry.file_name, document))
        .collect();
    Ok(MemorySnapshot {
        manifest,
        manifest_json,
        documents,
    })
}

/// Validates canonical manifest bytes, exact layout, digests, aliases, and bounds.
pub fn parse_memory_snapshot(
    manifest_json: &[u8],
    files: &[MemorySnapshotFile],
    limits: MemorySnapshotLimits,
) -> Result<MemorySnapshot, MemoryControlError> {
    if files.len() > limits.max_entries || manifest_json.len() > limits.max_total_bytes {
        return Err(MemoryControlError::BoundExceeded);
    }
    let total = files.iter().try_fold(manifest_json.len(), |total, file| {
        total
            .checked_add(file.bytes.len())
            .ok_or(MemoryControlError::BoundExceeded)
    })?;
    if total > limits.max_total_bytes {
        return Err(MemoryControlError::BoundExceeded);
    }
    let manifest: MemorySnapshotManifest =
        serde_json::from_slice(manifest_json).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    let canonical =
        serde_jcs::to_vec(&manifest).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    if canonical != manifest_json {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    validate_header(
        &manifest.export_id,
        &manifest.namespace_id,
        manifest.through_revision,
        &manifest.exported_at,
    )?;
    validate_unique_entries(&manifest.entries)?;
    let preimage = ManifestPreimage {
        schema_version: manifest.schema_version,
        export_id: &manifest.export_id,
        namespace_id: &manifest.namespace_id,
        through_revision: manifest.through_revision,
        exported_at: &manifest.exported_at,
        entries: &manifest.entries,
    };
    let digest = serde_jcs::to_vec(&preimage).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    if manifest.schema_version != 1 || manifest.manifest_digest != hex_sha256(&digest) {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let mut names = BTreeSet::new();
    let mut folded = BTreeSet::new();
    let mut storage = BTreeSet::new();
    let mut documents = Vec::with_capacity(files.len());
    for file in files {
        if !file.regular
            || file.storage_identity.is_empty()
            || !storage.insert(&file.storage_identity)
            || !names.insert(&file.file_name)
            || !folded.insert(file.file_name.to_lowercase())
            || !valid_file_name(&file.file_name)
        {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        let document = parse_memory_document(&file.bytes, limits.document)?;
        if document.render().as_bytes() != file.bytes {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        match document.record_ref() {
            MemoryRecordRef::Existing {
                record_id,
                revision_id,
            } => {
                let entry = manifest
                    .entries
                    .iter()
                    .find(|value| value.record_id == *record_id)
                    .ok_or(MemoryControlError::InvalidSnapshot)?;
                if entry.revision_id != *revision_id
                    || entry.file_name != file.file_name
                    || entry.file_name != expected_file_name(record_id)
                    || !entry_matches(entry, &document)
                {
                    return Err(MemoryControlError::InvalidSnapshot);
                }
            }
            MemoryRecordRef::New { draft_token } => {
                if file.file_name != format!("entries/new-{draft_token}.md") {
                    return Err(MemoryControlError::InvalidSnapshot);
                }
            }
        }
        documents.push((file.file_name.clone(), document));
    }
    if manifest
        .entries
        .iter()
        .any(|entry| !names.contains(&entry.file_name))
    {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    documents.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(MemorySnapshot {
        manifest,
        manifest_json: manifest_json.to_vec(),
        documents,
    })
}

fn validate_header(
    export_id: &str,
    namespace_id: &str,
    through_revision: u64,
    exported_at: &str,
) -> Result<(), MemoryControlError> {
    if export_id.is_empty()
        || namespace_id.is_empty()
        || through_revision == 0
        || chrono::DateTime::parse_from_rfc3339(exported_at).is_err()
    {
        Err(MemoryControlError::InvalidSnapshot)
    } else {
        Ok(())
    }
}

fn validate_unique_entries(entries: &[MemorySnapshotEntry]) -> Result<(), MemoryControlError> {
    let ordered = entries.windows(2).all(|pair| {
        (&pair[0].record_id, &pair[0].revision_id) < (&pair[1].record_id, &pair[1].revision_id)
    });
    let unique =
        |values: Vec<&str>| values.len() == values.into_iter().collect::<BTreeSet<_>>().len();
    if !ordered
        || !unique(entries.iter().map(|v| v.file_name.as_str()).collect())
        || !unique(entries.iter().map(|v| v.content_digest.as_str()).collect())
        || !unique(entries.iter().map(|v| v.document_digest.as_str()).collect())
    {
        Err(MemoryControlError::InvalidSnapshot)
    } else {
        Ok(())
    }
}

fn entry_matches(entry: &MemorySnapshotEntry, document: &MemoryControlDocument) -> bool {
    entry.authority == authority_name(document.authority())
        && entry.memory_type == type_name(document.memory_type())
        && entry.memory_role == role_name(document.memory_role())
        && entry.scope == scope_name(document.scope())
        && entry.scope_owner_id == document.scope_owner_id()
        && entry.lifecycle == lifecycle_name(document.lifecycle())
        && entry.sensitivity == sensitivity_name(document.sensitivity())
        && entry.content_digest == document.content_digest()
        && entry.document_digest == document.document_digest()
}

fn expected_file_name(record_id: &str) -> String {
    format!("entries/{}.md", hex_sha256(record_id.as_bytes()))
}
fn valid_file_name(value: &str) -> bool {
    value.starts_with("entries/")
        && value.ends_with(".md")
        && !value.contains("..")
        && !value.contains('\\')
        && value.matches('/').count() == 1
}
fn authority_name(value: MemoryAuthority) -> &'static str {
    match value {
        MemoryAuthority::UserDeclared => "user_declared",
        MemoryAuthority::AgentLearned => "agent_learned",
        MemoryAuthority::OrganisationPublished => "organisation_published",
    }
}
fn type_name(value: MemoryType) -> &'static str {
    match value {
        MemoryType::Semantic => "semantic",
        MemoryType::Episodic => "episodic",
        MemoryType::Lesson => "lesson",
        MemoryType::Procedural => "procedural",
    }
}
fn role_name(value: MemoryKind) -> &'static str {
    match value {
        MemoryKind::Preference => "preference",
        MemoryKind::Constraint => "constraint",
        MemoryKind::Decision => "decision",
        MemoryKind::LearnedFact => "learned_fact",
        MemoryKind::Summary => "summary",
    }
}
fn scope_name(value: MemoryScopeClass) -> &'static str {
    match value {
        MemoryScopeClass::Session => "session",
        MemoryScopeClass::AgentInstance => "agent_instance",
        MemoryScopeClass::User => "user",
        MemoryScopeClass::Project => "project",
        MemoryScopeClass::Platform => "platform",
    }
}
fn lifecycle_name(value: HypothesisState) -> &'static str {
    match value {
        HypothesisState::Candidate => "candidate",
        HypothesisState::Active => "active",
        HypothesisState::Cold => "cold",
        HypothesisState::Archived => "archived",
        HypothesisState::Promoted => "promoted",
    }
}
fn sensitivity_name(value: MemorySensitivity) -> &'static str {
    match value {
        MemorySensitivity::Ordinary => "ordinary",
        MemorySensitivity::Restricted => "restricted",
    }
}
