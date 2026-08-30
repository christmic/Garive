//! Pure M2 import planning over verified documents and current projection state.

use std::collections::BTreeSet;

use crate::{
    control_plan_values::{
        MemoryAuthorizedScope, MemoryCurrentEntry, MemoryIdentityAllocation, MemoryImportOperation,
        MemoryImportPlan, PlanPreimage,
    },
    control_plane::hex_sha256,
    HypothesisState, MemoryAuthority, MemoryControlDocument, MemoryControlError, MemoryRecordRef,
};

/// Produces one authority-safe, deterministic M2 import plan without I/O.
#[allow(clippy::too_many_arguments)]
pub fn prepare_memory_import(
    export_id: &str,
    namespace_id: &str,
    through_revision: u64,
    input_manifest_digest: &str,
    current_repository_revision: u64,
    documents: &[MemoryControlDocument],
    current: &[MemoryCurrentEntry],
    authorized_scopes: &[MemoryAuthorizedScope],
    allocations: &[MemoryIdentityAllocation],
) -> Result<MemoryImportPlan, MemoryControlError> {
    if export_id.is_empty()
        || namespace_id.is_empty()
        || through_revision == 0
        || input_manifest_digest.len() != 64
        || !input_manifest_digest
            .bytes()
            .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase())
    {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    if through_revision != current_repository_revision {
        return Err(MemoryControlError::StaleSnapshot);
    }
    if !ordered_current(current) || !ordered_scopes(authorized_scopes) {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let mut references = BTreeSet::new();
    for document in documents {
        let key = match document.record_ref() {
            MemoryRecordRef::Existing { record_id, .. } => format!("existing:{record_id}"),
            MemoryRecordRef::New { draft_token } => format!("new:{draft_token}"),
        };
        if !references.insert(key) {
            return Err(MemoryControlError::InvalidSnapshot);
        }
    }

    let mut operations = Vec::new();
    for document in documents {
        match document.record_ref() {
            MemoryRecordRef::New { draft_token } => plan_add(
                document,
                draft_token,
                authorized_scopes,
                allocations,
                &mut operations,
            )?,
            MemoryRecordRef::Existing {
                record_id,
                revision_id,
            } => {
                let entry = current
                    .iter()
                    .find(|value| value.record_id == *record_id)
                    .ok_or(MemoryControlError::StaleSnapshot)?;
                if entry.revision_id != *revision_id {
                    return Err(MemoryControlError::StaleSnapshot);
                }
                plan_existing(document, entry, allocations, &mut operations)?;
            }
        }
    }
    operations.sort_by(|left, right| {
        left.record_id()
            .cmp(right.record_id())
            .then(left.rank().cmp(&right.rank()))
    });
    if operations
        .windows(2)
        .any(|pair| pair[0].record_id() == pair[1].record_id())
    {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let add_count = count(&operations, |value| {
        matches!(value, MemoryImportOperation::Add { .. })
    });
    let supersede_count = count(&operations, |value| {
        matches!(value, MemoryImportOperation::Supersede { .. })
    });
    let archive_count = count(&operations, |value| {
        matches!(value, MemoryImportOperation::Archive { .. })
    });
    let erase_count = count(&operations, |value| {
        matches!(value, MemoryImportOperation::Erase { .. })
    });
    let preimage = PlanPreimage {
        schema_version: 1,
        export_id,
        namespace_id,
        through_revision,
        input_manifest_digest,
        expected_repository_revision: current_repository_revision,
        operations: &operations,
        add_count,
        supersede_count,
        archive_count,
        erase_count,
    };
    let canonical =
        serde_jcs::to_vec(&preimage).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    Ok(MemoryImportPlan {
        export_id: export_id.into(),
        namespace_id: namespace_id.into(),
        through_revision,
        input_manifest_digest: input_manifest_digest.into(),
        expected_repository_revision: current_repository_revision,
        operations,
        add_count,
        supersede_count,
        archive_count,
        erase_count,
        plan_digest: hex_sha256(&canonical),
    })
}

fn count(
    operations: &[MemoryImportOperation],
    predicate: impl Fn(&MemoryImportOperation) -> bool,
) -> u64 {
    operations.iter().filter(|value| predicate(value)).count() as u64
}

fn plan_add(
    document: &MemoryControlDocument,
    draft_token: &str,
    authorized_scopes: &[MemoryAuthorizedScope],
    allocations: &[MemoryIdentityAllocation],
    operations: &mut Vec<MemoryImportOperation>,
) -> Result<(), MemoryControlError> {
    if document.authority() != MemoryAuthority::UserDeclared
        || document.lifecycle() != HypothesisState::Active
        || document.erase_requested()
        || !authorized_scopes.iter().any(|value| {
            value.scope == document.scope() && value.owner_id == document.scope_owner_id()
        })
    {
        return Err(MemoryControlError::ForbiddenChange);
    }
    let matches: Vec<_> = allocations
        .iter()
        .filter_map(|value| match value {
            MemoryIdentityAllocation::Add {
                draft_token: source,
                record_id,
                revision_id,
            } if source == draft_token => Some((record_id, revision_id)),
            _ => None,
        })
        .collect();
    let [(record_id, revision_id)] = matches.as_slice() else {
        return Err(MemoryControlError::InvalidSnapshot);
    };
    operations.push(MemoryImportOperation::Add {
        source_draft_token: draft_token.into(),
        record_id: (*record_id).clone(),
        revision_id: (*revision_id).clone(),
        expected_absent: true,
        document_digest: document.document_digest(),
    });
    Ok(())
}

fn plan_existing(
    document: &MemoryControlDocument,
    entry: &MemoryCurrentEntry,
    allocations: &[MemoryIdentityAllocation],
    operations: &mut Vec<MemoryImportOperation>,
) -> Result<(), MemoryControlError> {
    if document.memory_type() != entry.memory_type
        || document.memory_role() != entry.memory_role
        || document.scope() != entry.scope
        || document.scope_owner_id() != entry.scope_owner_id
        || document.sensitivity() != entry.sensitivity
    {
        return Err(MemoryControlError::ForbiddenChange);
    }
    if document.erase_requested() {
        if document.content_digest() != entry.content_digest
            || document.lifecycle() != entry.lifecycle
            || document.authority() != entry.authority
            || entry.authority == MemoryAuthority::OrganisationPublished
        {
            return Err(MemoryControlError::ForbiddenChange);
        }
        operations.push(MemoryImportOperation::Erase {
            record_id: entry.record_id.clone(),
            expected_active_revision_id: entry.revision_id.clone(),
            document_digest: document.document_digest(),
        });
    } else if document.lifecycle() != entry.lifecycle {
        if document.lifecycle() != HypothesisState::Archived
            || document.content_digest() != entry.content_digest
            || document.authority() != entry.authority
            || entry.authority == MemoryAuthority::OrganisationPublished
        {
            return Err(MemoryControlError::ForbiddenChange);
        }
        operations.push(MemoryImportOperation::Archive {
            record_id: entry.record_id.clone(),
            expected_active_revision_id: entry.revision_id.clone(),
            document_digest: document.document_digest(),
        });
    } else if document.content_digest() != entry.content_digest {
        if entry.authority == MemoryAuthority::OrganisationPublished
            || document.authority() != MemoryAuthority::UserDeclared
        {
            return Err(MemoryControlError::ForbiddenChange);
        }
        let matches: Vec<_> = allocations
            .iter()
            .filter_map(|value| match value {
                MemoryIdentityAllocation::Supersede {
                    record_id,
                    revision_id,
                } if record_id == &entry.record_id => Some(revision_id),
                _ => None,
            })
            .collect();
        let [new_revision_id] = matches.as_slice() else {
            return Err(MemoryControlError::InvalidSnapshot);
        };
        operations.push(MemoryImportOperation::Supersede {
            record_id: entry.record_id.clone(),
            expected_active_revision_id: entry.revision_id.clone(),
            new_revision_id: (*new_revision_id).clone(),
            authority: MemoryAuthority::UserDeclared,
            document_digest: document.document_digest(),
            supersedes_learned_revision_id: (entry.authority == MemoryAuthority::AgentLearned)
                .then(|| entry.revision_id.clone()),
        });
    } else if document.authority() != entry.authority {
        return Err(MemoryControlError::ForbiddenChange);
    }
    Ok(())
}

fn ordered_current(values: &[MemoryCurrentEntry]) -> bool {
    values
        .windows(2)
        .all(|pair| pair[0].record_id < pair[1].record_id)
}

fn ordered_scopes(values: &[MemoryAuthorizedScope]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
