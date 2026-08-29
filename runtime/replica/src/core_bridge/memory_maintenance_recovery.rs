use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{DurableFact, FactKind};
use garive_memory::{
    advance_distillation, DistillationWatermark, ErasureDisposition, MemoryErrorCode,
};
use serde_json::{Map, Value};

use crate::SqliteLedger;

use super::memory_maintenance_projection::{
    MemoryMaintenanceProjection, RecordedMemoryDecision, RecordedMemoryErasure,
};
use super::MemoryPrefix;

#[derive(Clone)]
struct CandidateFact {
    session: String,
    position: u64,
}
#[derive(Clone)]
struct PromotionRequest {
    record_id: String,
    revision_id: String,
    proposal_id: String,
}
#[derive(Clone)]
struct PromotionReceipt {
    session: String,
    position: u64,
    record_id: String,
    revision_id: String,
    digest: String,
}
#[derive(Clone)]
struct ErasureRequest {
    record_id: String,
    revision_id: String,
    targets: Vec<String>,
}

/// Rebuilds M1 maintenance state and rejects every torn or mismatched pair.
pub fn reconstruct_memory_maintenance_projection(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
    namespace_id: &str,
) -> Result<MemoryMaintenanceProjection, MemoryErrorCode> {
    if namespace_id.is_empty()
        || prefixes.is_empty()
        || prefixes.iter().any(|item| item.through_position == 0)
        || !prefixes
            .windows(2)
            .all(|pair| pair[0].session_id < pair[1].session_id)
    {
        return Err(MemoryErrorCode::InvalidMemory);
    }
    let kinds = maintenance_kinds();
    let mut facts = Vec::new();
    for prefix in prefixes {
        facts.extend(
            ledger
                .read_facts(&prefix.session_id, 0, prefix.through_position, Some(&kinds))
                .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        );
    }
    let coordinates: BTreeMap<_, _> = facts
        .iter()
        .map(|fact| ((fact.session_id.as_str().to_owned(), fact.position), fact))
        .collect();
    let mut candidates = BTreeMap::new();
    let mut matched_candidates = BTreeSet::new();
    let mut decisions = Vec::new();
    let mut watermarks: BTreeMap<(String, String), DistillationWatermark> = BTreeMap::new();
    let mut promotion_requests = BTreeMap::new();
    let mut promotion_receipts = BTreeMap::new();
    let mut matched_promotions = BTreeSet::new();
    let mut promoted = BTreeSet::new();
    let mut erasure_requests = BTreeMap::new();
    let mut erasures = BTreeMap::new();
    let mut audit_count = 0_u64;
    for fact in &facts {
        let payload: Value = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
        let object = payload
            .as_object()
            .ok_or(MemoryErrorCode::CorruptMemoryState)?;
        if fact.kind.as_str() != "memory.tombstoned"
            && text(object, "namespace_id")? != namespace_id
        {
            continue;
        }
        match fact.kind.as_str() {
            "memory.candidate_recorded" => {
                let id = text(object, "candidate_id")?;
                if candidates
                    .insert(
                        id,
                        CandidateFact {
                            session: fact.session_id.as_str().into(),
                            position: fact.position,
                        },
                    )
                    .is_some()
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
            }
            "memory.maintenance_decided" => {
                let candidate_id = text(object, "candidate_id")?;
                let candidate = candidates
                    .get(&candidate_id)
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?;
                if candidate.session != fact.session_id.as_str()
                    || candidate.position.checked_add(1) != Some(fact.position)
                    || !matched_candidates.insert(candidate_id.clone())
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
                decisions.push(RecordedMemoryDecision {
                    candidate_id,
                    decision_kind: text(object, "decision_kind")?,
                });
            }
            "memory.distillation_checkpointed" => {
                let watermark = DistillationWatermark::new(
                    text(object, "extractor_revision")?,
                    text(object, "session_id")?,
                    number(object, "through_position")?,
                    text(object, "batch_digest")?,
                )
                .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
                let key = (
                    watermark.extractor_revision.clone(),
                    watermark.session_id.clone(),
                );
                advance_distillation(watermarks.get(&key), &watermark)
                    .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
                watermarks.insert(key, watermark);
            }
            "memory.audit_recorded" => {
                audit_count = audit_count
                    .checked_add(1)
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?
            }
            "memory.promotion_requested" => {
                let id = text(object, "request_id")?;
                let request = PromotionRequest {
                    record_id: text(object, "record_id")?,
                    revision_id: text(object, "revision_id")?,
                    proposal_id: text(object, "knowledge_proposal_id")?,
                };
                if promotion_requests.insert(id, request).is_some() {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
            }
            "memory.promotion_recorded" => {
                let id = text(object, "request_id")?;
                let request = promotion_requests
                    .get(&id)
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?;
                if request.record_id != text(object, "record_id")?
                    || request.revision_id != text(object, "revision_id")?
                    || request.proposal_id != text(object, "knowledge_proposal_id")?
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
                let receipt = PromotionReceipt {
                    session: fact.session_id.as_str().into(),
                    position: fact.position,
                    record_id: request.record_id.clone(),
                    revision_id: request.revision_id.clone(),
                    digest: text(object, "receipt_digest")?,
                };
                if promotion_receipts.insert(id, receipt).is_some() {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
            }
            "memory.lifecycle_transitioned"
                if object.get("cause_kind").and_then(Value::as_str) == Some("promotion") =>
            {
                let id = text(object, "cause_id")?;
                let receipt = promotion_receipts
                    .get(&id)
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?;
                if receipt.session != fact.session_id.as_str()
                    || receipt.position.checked_add(1) != Some(fact.position)
                    || receipt.record_id != text(object, "record_id")?
                    || receipt.revision_id != text(object, "revision_id")?
                    || receipt.digest != text(object, "promoted_knowledge_receipt_digest")?
                    || text(object, "to_state")? != "promoted"
                    || !matched_promotions.insert(id)
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
                promoted.insert((receipt.record_id.clone(), receipt.revision_id.clone()));
            }
            "memory.erasure_requested" => {
                validate_tombstone_reference(fact, object, &coordinates)?;
                let id = text(object, "request_id")?;
                let targets = object["targets"]
                    .as_array()
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?
                    .iter()
                    .map(|item| {
                        text(
                            item.as_object()
                                .ok_or(MemoryErrorCode::CorruptMemoryState)?,
                            "target_id",
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let request = ErasureRequest {
                    record_id: text(object, "record_id")?,
                    revision_id: text(object, "revision_id")?,
                    targets,
                };
                if erasure_requests.insert(id, request).is_some() {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
            }
            "memory.erasure_recorded" => {
                let id = text(object, "request_id")?;
                let request = erasure_requests
                    .get(&id)
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?;
                if request.record_id != text(object, "record_id")?
                    || request.revision_id != text(object, "revision_id")?
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
                let results = object["results"]
                    .as_array()
                    .ok_or(MemoryErrorCode::CorruptMemoryState)?;
                let ids = results
                    .iter()
                    .map(|item| {
                        text(
                            item.as_object()
                                .ok_or(MemoryErrorCode::CorruptMemoryState)?,
                            "target_id",
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if ids != request.targets {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
                let pending_targets = results
                    .iter()
                    .filter_map(|item| {
                        let item = item.as_object()?;
                        (!matches!(item.get("status")?.as_str()?, "erased" | "not_present"))
                            .then(|| item.get("target_id")?.as_str().map(str::to_owned))
                            .flatten()
                    })
                    .collect();
                let disposition = match text(object, "disposition")?.as_str() {
                    "complete" => ErasureDisposition::Complete,
                    "partial" => ErasureDisposition::Partial,
                    _ => return Err(MemoryErrorCode::CorruptMemoryState),
                };
                let attempted_at_position = number(object, "attempted_at_position")?;
                if erasures
                    .get(&id)
                    .is_some_and(|prior: &RecordedMemoryErasure| {
                        prior.attempted_at_position >= attempted_at_position
                    })
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
                erasures.insert(
                    id.clone(),
                    RecordedMemoryErasure {
                        request_id: id,
                        attempt_id: text(object, "attempt_id")?,
                        attempted_at_position,
                        disposition,
                        pending_targets,
                    },
                );
            }
            "memory.tombstoned" | "memory.lifecycle_transitioned" => {}
            _ => return Err(MemoryErrorCode::CorruptMemoryState),
        }
    }
    if matched_candidates.len() != candidates.len()
        || matched_promotions.len() != promotion_receipts.len()
    {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    Ok(MemoryMaintenanceProjection {
        namespace_id: namespace_id.into(),
        decisions,
        watermarks,
        promoted,
        erasures,
        audit_count,
    })
}

fn validate_tombstone_reference(
    request_fact: &DurableFact,
    value: &Map<String, Value>,
    coordinates: &BTreeMap<(String, u64), &DurableFact>,
) -> Result<(), MemoryErrorCode> {
    let reference = value["tombstone_fact"]
        .as_object()
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    let session = text(reference, "session_id")?;
    let position = number(reference, "position")?;
    let tombstone = coordinates
        .get(&(session, position))
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    let payload: Value = serde_json::from_str(tombstone.payload.as_json())
        .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
    let tombstone_object = payload
        .as_object()
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    if tombstone.kind.as_str() != "memory.tombstoned"
        || request_fact.session_id != tombstone.session_id
        || tombstone.position.checked_add(1) != Some(request_fact.position)
        || tombstone.fact_id.as_str() != text(reference, "fact_id")?
        || tombstone.payload.sha256() != text(reference, "payload_digest")?
        || text(tombstone_object, "record_id")? != text(value, "record_id")?
        || text(tombstone_object, "revision_id")? != text(value, "revision_id")?
    {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    Ok(())
}

fn text(value: &Map<String, Value>, key: &str) -> Result<String, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn number(value: &Map<String, Value>, key: &str) -> Result<u64, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn maintenance_kinds() -> BTreeSet<FactKind> {
    [
        "memory.tombstoned",
        "memory.candidate_recorded",
        "memory.maintenance_decided",
        "memory.distillation_checkpointed",
        "memory.audit_recorded",
        "memory.promotion_requested",
        "memory.promotion_recorded",
        "memory.lifecycle_transitioned",
        "memory.erasure_requested",
        "memory.erasure_recorded",
    ]
    .into_iter()
    .map(|value| FactKind::new(value).expect("constant kind"))
    .collect()
}
