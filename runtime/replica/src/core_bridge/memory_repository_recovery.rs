use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{DurableFact, FactKind};
use garive_memory::{
    EvidenceTally, HypothesisState, MemoryAuthority, MemoryControlDocument, MemoryDocumentLimits,
    MemoryErrorCode, MemoryLifecycle, MemoryScope, MemoryScopeClass, MemoryStatus, MemoryType,
};
use serde_json::Value;

use crate::{MemoryControlProjection, SqliteLedger};

use super::{
    memory_hypothesis_recovery::reconstruct_memory_hypothesis_projection,
    memory_recovery::{reconstruct_memory_state, MemoryPrefix},
};

/// Independently recovered repository view and exact lifecycle reducers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredMemoryRepository {
    /// Canonical active M2 projection.
    pub projection: MemoryControlProjection,
    lifecycles: BTreeMap<(String, String), MemoryLifecycle>,
}

impl RecoveredMemoryRepository {
    /// Returns the complete lifecycle for one classified revision.
    pub fn lifecycle(&self, record_id: &str, revision_id: &str) -> Option<&MemoryLifecycle> {
        self.lifecycles.get(&(record_id.into(), revision_id.into()))
    }
}

/// Independently rebuilds the canonical current repository from authorized fixed fact prefixes.
pub fn reconstruct_memory_repository_projection(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
    namespace_id: &str,
    limits: MemoryDocumentLimits,
) -> Result<MemoryControlProjection, MemoryErrorCode> {
    reconstruct_memory_repository(ledger, prefixes, namespace_id, limits)
        .map(|recovered| recovered.projection)
}

/// Recovers both the visible repository and exact lifecycle state from fixed facts.
pub fn reconstruct_memory_repository(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
    namespace_id: &str,
    limits: MemoryDocumentLimits,
) -> Result<RecoveredMemoryRepository, MemoryErrorCode> {
    let state = reconstruct_memory_state(ledger, prefixes)?;
    let hypothesis = reconstruct_memory_hypothesis_projection(ledger, prefixes, namespace_id)?;
    let kinds = BTreeSet::from([
        FactKind::new("memory.committed").unwrap(),
        FactKind::new("memory.revision_classified").unwrap(),
        FactKind::new("memory.tombstoned").unwrap(),
        FactKind::new("memory.lifecycle_transitioned").unwrap(),
    ]);
    let mut commits = BTreeMap::new();
    let mut classifications = BTreeMap::new();
    let mut change_batches = BTreeSet::new();
    for prefix in prefixes {
        for fact in ledger
            .read_facts(&prefix.session_id, 0, prefix.through_position, Some(&kinds))
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)?
        {
            let value: Value = serde_json::from_str(fact.payload.as_json())
                .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
            if text(&value, "namespace_id")? != namespace_id {
                continue;
            }
            if matches!(
                fact.kind.as_str(),
                "memory.tombstoned" | "memory.lifecycle_transitioned"
            ) {
                record_change_batch(ledger, &fact, &mut change_batches)?;
                continue;
            }
            let key = (text(&value, "record_id")?, text(&value, "revision_id")?);
            match fact.kind.as_str() {
                "memory.committed" => {
                    if commits.insert(key, (fact, value)).is_some() {
                        return Err(MemoryErrorCode::CorruptMemoryState);
                    }
                }
                "memory.revision_classified" => {
                    record_change_batch(ledger, &fact, &mut change_batches)?;
                    if classifications.insert(key, (fact, value)).is_some() {
                        return Err(MemoryErrorCode::CorruptMemoryState);
                    }
                }
                _ => return Err(MemoryErrorCode::CorruptMemoryState),
            }
        }
    }
    if commits.len() != classifications.len() || commits.keys().ne(classifications.keys()) {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    let mut documents = Vec::new();
    let mut lifecycles = BTreeMap::new();
    for record in state.revisions() {
        if record.namespace_id() != namespace_id || record.status() != MemoryStatus::Active {
            continue;
        }
        let key = (
            record.record_id().to_owned(),
            record.revision_id().to_owned(),
        );
        let (commit_fact, _) = commits
            .get(&key)
            .ok_or(MemoryErrorCode::CorruptMemoryState)?;
        let (_, classification) = classifications
            .get(&key)
            .ok_or(MemoryErrorCode::CorruptMemoryState)?;
        verify_source(commit_fact, classification)?;
        let classified_scope = scope(text(classification, "scope")?)?;
        let owner = text(classification, "scope_owner_id")?;
        if !scope_matches(record.scope(), classified_scope, &owner) {
            return Err(MemoryErrorCode::CorruptMemoryState);
        }
        let initial = lifecycle(text(classification, "lifecycle")?)?;
        let initial_lifecycle = MemoryLifecycle::new(
            initial,
            EvidenceTally {
                verified: 0,
                falsified: 0,
                neutral: 0,
            },
            commit_fact.position,
            None,
        )
        .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
        let lifecycle = match hypothesis.lifecycle(record.record_id(), record.revision_id()) {
            Some(value)
                if hypothesis.initial_state(record.record_id(), record.revision_id())
                    == Some(initial)
                    && value.last_observed_position() > commit_fact.position =>
            {
                value.clone()
            }
            Some(_) => return Err(MemoryErrorCode::CorruptMemoryState),
            None => initial_lifecycle,
        };
        let content = record
            .content()
            .inline_utf8()
            .ok_or(MemoryErrorCode::CorruptMemoryState)?;
        documents.push(
            MemoryControlDocument::from_repository_record(
                record.record_id(),
                record.revision_id(),
                authority(text(classification, "authority")?)?,
                memory_type(text(classification, "memory_type")?)?,
                record.kind(),
                classified_scope,
                owner,
                lifecycle.state(),
                record.sensitivity(),
                content,
                limits,
            )
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        );
        lifecycles.insert(key, lifecycle);
    }
    documents.sort_by(|left, right| {
        left.record_ref()
            .record_id()
            .cmp(&right.record_ref().record_id())
    });
    Ok(RecoveredMemoryRepository {
        projection: MemoryControlProjection {
            namespace_id: namespace_id.into(),
            repository_revision: u64::try_from(change_batches.len())
                .ok()
                .filter(|value| *value != 0)
                .ok_or(MemoryErrorCode::CorruptMemoryState)?,
            documents,
        },
        lifecycles,
    })
}

fn record_change_batch(
    ledger: &SqliteLedger,
    fact: &DurableFact,
    batches: &mut BTreeSet<(String, u64)>,
) -> Result<(), MemoryErrorCode> {
    let version = ledger
        .fact_commit_version(&fact.fact_id)
        .map_err(|_| MemoryErrorCode::CorruptMemoryState)?
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    if version == 0 {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    batches.insert((fact.session_id.as_str().into(), version));
    Ok(())
}

fn verify_source(fact: &DurableFact, classification: &Value) -> Result<(), MemoryErrorCode> {
    let source = classification
        .get("source_commit")
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    if text(source, "session_id")? != fact.session_id.as_str()
        || number(source, "position")? != fact.position
        || text(source, "fact_id")? != fact.fact_id.as_str()
        || text(source, "payload_digest")? != fact.payload.sha256()
    {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    Ok(())
}

fn scope_matches(source: &MemoryScope, target: MemoryScopeClass, owner: &str) -> bool {
    match source {
        MemoryScope::Session { owner_id } => {
            target == MemoryScopeClass::Session && owner == owner_id
        }
        MemoryScope::AgentInstance { owner_id } => {
            target == MemoryScopeClass::AgentInstance && owner == owner_id
        }
        MemoryScope::Namespace => matches!(
            target,
            MemoryScopeClass::User | MemoryScopeClass::Project | MemoryScopeClass::Platform
        ),
    }
}

fn text(value: &Value, key: &str) -> Result<String, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn number(value: &Value, key: &str) -> Result<u64, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn authority(value: String) -> Result<MemoryAuthority, MemoryErrorCode> {
    match value.as_str() {
        "user_declared" => Ok(MemoryAuthority::UserDeclared),
        "agent_learned" => Ok(MemoryAuthority::AgentLearned),
        "organisation_published" => Ok(MemoryAuthority::OrganisationPublished),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
fn memory_type(value: String) -> Result<MemoryType, MemoryErrorCode> {
    match value.as_str() {
        "semantic" => Ok(MemoryType::Semantic),
        "episodic" => Ok(MemoryType::Episodic),
        "lesson" => Ok(MemoryType::Lesson),
        "procedural" => Ok(MemoryType::Procedural),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
fn scope(value: String) -> Result<MemoryScopeClass, MemoryErrorCode> {
    match value.as_str() {
        "session" => Ok(MemoryScopeClass::Session),
        "agent_instance" => Ok(MemoryScopeClass::AgentInstance),
        "user" => Ok(MemoryScopeClass::User),
        "project" => Ok(MemoryScopeClass::Project),
        "platform" => Ok(MemoryScopeClass::Platform),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
fn lifecycle(value: String) -> Result<HypothesisState, MemoryErrorCode> {
    match value.as_str() {
        "candidate" => Ok(HypothesisState::Candidate),
        "active" => Ok(HypothesisState::Active),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
