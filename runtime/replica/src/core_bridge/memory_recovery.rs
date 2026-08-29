use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{FactKind, SessionId};
use garive_memory::{
    ContentBinding, DurableFactReference, MemoryErrorCode, MemoryKind, MemoryProposal,
    MemoryRecord, MemoryScope, MemorySensitivity, MemoryState, MemoryStatus,
};
use serde_json::{Map, Value};

use crate::{SqliteLedger, SqliteLedgerError};

/// One Runtime-authorized Session prefix participating in Memory recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryPrefix {
    /// Session whose Memory facts and evidence may be read.
    pub session_id: SessionId,
    /// Inclusive fixed durable position.
    pub through_position: u64,
}

/// Reconstructs immutable Memory state from a canonical authorized prefix set.
pub fn reconstruct_memory_state(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
) -> Result<MemoryState, MemoryErrorCode> {
    validate_prefixes(prefixes)?;
    let kinds = memory_kinds();
    let mut commits = Vec::new();
    let mut superseded = BTreeSet::new();
    let mut tombstoned = BTreeSet::new();
    for prefix in prefixes {
        let facts = ledger
            .read_facts(&prefix.session_id, 0, prefix.through_position, Some(&kinds))
            .map_err(map_ledger)?;
        for fact in facts {
            let value: Value = serde_json::from_str(fact.payload.as_json())
                .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
            let object = value
                .as_object()
                .ok_or(MemoryErrorCode::CorruptMemoryState)?;
            match fact.kind.as_str() {
                "memory.committed" => {
                    if let Some(prior) = optional_text(object, "supersedes_revision_id")? {
                        superseded.insert((text(object, "record_id")?, prior));
                    }
                    commits.push(object.clone());
                }
                "memory.tombstoned" => {
                    tombstoned.insert((text(object, "record_id")?, text(object, "revision_id")?));
                }
                _ => {}
            }
        }
    }
    if superseded
        .iter()
        .any(|identity| tombstoned.contains(identity))
    {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    let identities = commits
        .iter()
        .map(|value| Ok((text(value, "record_id")?, text(value, "revision_id")?)))
        .collect::<Result<BTreeSet<_>, MemoryErrorCode>>()?;
    if superseded
        .iter()
        .chain(tombstoned.iter())
        .any(|identity| !identities.contains(identity))
    {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    let records = commits
        .iter()
        .map(|value| {
            let identity = (text(value, "record_id")?, text(value, "revision_id")?);
            let status = if superseded.contains(&identity) {
                MemoryStatus::Superseded
            } else if tombstoned.contains(&identity) {
                MemoryStatus::Tombstoned
            } else {
                MemoryStatus::Active
            };
            record(value, status)
        })
        .collect::<Result<Vec<_>, MemoryErrorCode>>()?;
    MemoryState::new(records).map_err(|_| MemoryErrorCode::CorruptMemoryState)
}

/// Verifies every proposal evidence coordinate against authorized fixed prefixes.
pub fn verify_memory_evidence(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
    proposal: &MemoryProposal,
) -> Result<(), MemoryErrorCode> {
    validate_prefixes(prefixes)?;
    let positions: BTreeMap<_, _> = prefixes
        .iter()
        .map(|value| (value.session_id.as_str(), value.through_position))
        .collect();
    for reference in proposal.evidence() {
        let through = positions
            .get(reference.session_id())
            .ok_or(MemoryErrorCode::NamespaceDenied)?;
        if reference.position() > *through {
            return Err(MemoryErrorCode::EvidenceNotFound);
        }
        let session = SessionId::try_from(reference.session_id())
            .map_err(|_| MemoryErrorCode::InvalidMemory)?;
        let facts = ledger
            .read_facts(
                &session,
                reference.position() - 1,
                reference.position(),
                None,
            )
            .map_err(map_ledger)?;
        let Some(fact) = facts.first() else {
            return Err(MemoryErrorCode::EvidenceNotFound);
        };
        if fact.fact_id.as_str() != reference.fact_id()
            || fact.payload.sha256() != reference.payload_digest()
        {
            return Err(MemoryErrorCode::EvidenceMismatch);
        }
    }
    Ok(())
}

fn record(
    value: &Map<String, Value>,
    status: MemoryStatus,
) -> Result<MemoryRecord, MemoryErrorCode> {
    MemoryRecord::new(
        text(value, "record_id")?,
        text(value, "revision_id")?,
        text(value, "namespace_id")?,
        scope(value.get("scope"))?,
        kind(&text(value, "kind")?)?,
        content(value.get("content"))?,
        evidence(value.get("evidence"))?,
        status,
        sensitivity(&text(value, "sensitivity")?)?,
        number(value, "confidence_basis_points")?
            .try_into()
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        number(value, "valid_from_position")?,
        optional_text(value, "supersedes_revision_id")?,
        optional_text(value, "expires_at_utc")?,
    )
    .map_err(|_| MemoryErrorCode::CorruptMemoryState)
}

fn content(value: Option<&Value>) -> Result<ContentBinding, MemoryErrorCode> {
    let value = value
        .and_then(Value::as_object)
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    let digest = text(value, "digest")?;
    match (
        value.get("inline_utf8").and_then(Value::as_str),
        value.get("reference").and_then(Value::as_str),
    ) {
        (Some(inline), None) => {
            ContentBinding::inline(digest, inline).map_err(|_| MemoryErrorCode::CorruptMemoryState)
        }
        (None, Some(reference)) => ContentBinding::referenced(digest, reference)
            .map_err(|_| MemoryErrorCode::CorruptMemoryState),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}

fn evidence(value: Option<&Value>) -> Result<Vec<DurableFactReference>, MemoryErrorCode> {
    value
        .and_then(Value::as_array)
        .ok_or(MemoryErrorCode::CorruptMemoryState)?
        .iter()
        .map(|item| {
            let item = item
                .as_object()
                .ok_or(MemoryErrorCode::CorruptMemoryState)?;
            DurableFactReference::new(
                text(item, "session_id")?,
                number(item, "position")?,
                text(item, "fact_id")?,
                text(item, "payload_digest")?,
            )
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)
        })
        .collect()
}

fn scope(value: Option<&Value>) -> Result<MemoryScope, MemoryErrorCode> {
    let value = value
        .and_then(Value::as_object)
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    match text(value, "kind")?.as_str() {
        "session" => MemoryScope::session(text(value, "owner_id")?)
            .map_err(|_| MemoryErrorCode::CorruptMemoryState),
        "agent_instance" => MemoryScope::agent_instance(text(value, "owner_id")?)
            .map_err(|_| MemoryErrorCode::CorruptMemoryState),
        "namespace" => Ok(MemoryScope::Namespace),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}

fn kind(value: &str) -> Result<MemoryKind, MemoryErrorCode> {
    match value {
        "preference" => Ok(MemoryKind::Preference),
        "constraint" => Ok(MemoryKind::Constraint),
        "decision" => Ok(MemoryKind::Decision),
        "learned_fact" => Ok(MemoryKind::LearnedFact),
        "summary" => Ok(MemoryKind::Summary),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
fn sensitivity(value: &str) -> Result<MemorySensitivity, MemoryErrorCode> {
    match value {
        "ordinary" => Ok(MemorySensitivity::Ordinary),
        "restricted" => Ok(MemorySensitivity::Restricted),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
fn text(value: &Map<String, Value>, key: &str) -> Result<String, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn optional_text(value: &Map<String, Value>, key: &str) -> Result<Option<String>, MemoryErrorCode> {
    value.get(key).map(|_| text(value, key)).transpose()
}
fn number(value: &Map<String, Value>, key: &str) -> Result<u64, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn validate_prefixes(values: &[MemoryPrefix]) -> Result<(), MemoryErrorCode> {
    if values.is_empty()
        || values.iter().any(|value| value.through_position == 0)
        || !values
            .windows(2)
            .all(|pair| pair[0].session_id < pair[1].session_id)
    {
        Err(MemoryErrorCode::InvalidMemory)
    } else {
        Ok(())
    }
}
fn memory_kinds() -> BTreeSet<FactKind> {
    ["memory.committed", "memory.tombstoned"]
        .into_iter()
        .map(|value| FactKind::new(value).expect("constant kind"))
        .collect()
}
fn map_ledger(value: SqliteLedgerError) -> MemoryErrorCode {
    match value {
        SqliteLedgerError::Domain(garive_ledger::LedgerError::InvalidReadRange) => {
            MemoryErrorCode::EvidenceNotFound
        }
        _ => MemoryErrorCode::CorruptMemoryState,
    }
}
