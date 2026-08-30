use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{DurableFact, FactKind};
use garive_memory::{
    HypothesisState, MemoryAuthority, MemoryControlDocument, MemoryDocumentLimits, MemoryErrorCode,
    MemoryScope, MemoryScopeClass, MemoryStatus, MemoryType,
};
use serde_json::Value;

use crate::{MemoryControlProjection, SqliteLedger};

use super::{
    memory_hypothesis_recovery::reconstruct_memory_hypothesis_projection,
    memory_recovery::{reconstruct_memory_state, MemoryPrefix},
};

/// Independently rebuilds the canonical current repository from authorized fixed fact prefixes.
pub fn reconstruct_memory_repository_projection(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
    namespace_id: &str,
    limits: MemoryDocumentLimits,
) -> Result<MemoryControlProjection, MemoryErrorCode> {
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
    let mut transition_count = 0u64;
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
                transition_count = transition_count
                    .checked_add(1)
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?;
                continue;
            }
            let key = (text(&value, "record_id")?, text(&value, "revision_id")?);
            let target = match fact.kind.as_str() {
                "memory.committed" => &mut commits,
                "memory.revision_classified" => &mut classifications,
                _ => return Err(MemoryErrorCode::CorruptMemoryState),
            };
            if target.insert(key, (fact, value)).is_some() {
                return Err(MemoryErrorCode::CorruptMemoryState);
            }
        }
    }
    if commits.len() != classifications.len() || commits.keys().ne(classifications.keys()) {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    let mut documents = Vec::new();
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
        let lifecycle = hypothesis
            .lifecycle(record.record_id(), record.revision_id())
            .map(|value| value.state())
            .unwrap_or(initial);
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
                lifecycle,
                record.sensitivity(),
                content,
                limits,
            )
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        );
    }
    documents.sort_by(|left, right| {
        left.record_ref()
            .record_id()
            .cmp(&right.record_ref().record_id())
    });
    Ok(MemoryControlProjection {
        namespace_id: namespace_id.into(),
        repository_revision: (classifications.len() as u64)
            .checked_add(transition_count)
            .ok_or(MemoryErrorCode::CorruptMemoryState)?,
        documents,
    })
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
