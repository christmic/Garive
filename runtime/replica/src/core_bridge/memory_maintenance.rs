use chrono::{DateTime, SecondsFormat, Utc};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_memory::{
    complete_memory_promotion, DistillationWatermark, ErasureDisposition, ErasureTargetKind,
    ErasureTargetStatus, HypothesisState, MemoryAuditReport, MemoryCandidate,
    MemoryCandidateIntent, MemoryCandidateSource, MemoryErasureReceipt, MemoryErasureRequest,
    MemoryLifecycle, MemoryMaintenanceDecision, MemoryPromotionReceipt, MemoryPromotionRequest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::runtime_turn::RuntimeCommandError;

use super::PlannedMemoryTombstone;

/// Session, namespace, and time ownership for asynchronous Memory maintenance facts.
pub struct MemoryMaintenanceContext {
    /// Session receiving the durable facts.
    pub session_id: SessionId,
    /// Authorized opaque namespace.
    pub namespace_id: String,
    /// Canonical Runtime observation time.
    pub recorded_at: String,
}

/// Candidate and its exact four-way decision, committed atomically.
pub fn plan_memory_maintenance_decision(
    context: &MemoryMaintenanceContext,
    candidate: &MemoryCandidate,
    decision: &MemoryMaintenanceDecision,
    intent_digest: &str,
    decision_digest: &str,
) -> Result<Vec<FactDraft>, RuntimeCommandError> {
    validate_context(context)?;
    validate_digest(intent_digest)?;
    validate_digest(decision_digest)?;
    if candidate.namespace_id() != context.namespace_id {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let decision_id = match decision {
        MemoryMaintenanceDecision::Add { proposal_id }
        | MemoryMaintenanceDecision::Update { proposal_id, .. } => proposal_id,
        MemoryMaintenanceDecision::Delete { command_id, .. } => command_id,
        MemoryMaintenanceDecision::Noop { .. } => candidate.candidate_id(),
    };
    let candidate_fact = fact(
        "memory.candidate_recorded",
        candidate.candidate_id(),
        json!({"candidate_id": candidate.candidate_id(), "namespace_id": context.namespace_id,
            "extractor_revision": candidate.extractor_revision(), "source": source(candidate.source()),
            "intent_kind": match candidate.intent() { MemoryCandidateIntent::Learn { .. } => "learn", MemoryCandidateIntent::Forget { .. } => "forget" },
            "intent_digest": intent_digest}),
        &context.recorded_at,
    )?;
    let decision_fact = fact(
        "memory.maintenance_decided",
        decision_id,
        json!({"decision_id": decision_id, "candidate_id": candidate.candidate_id(),
            "namespace_id": context.namespace_id, "decision_kind": decision_kind(decision),
            "decision_digest": decision_digest}),
        &context.recorded_at,
    )?;
    Ok(vec![candidate_fact, decision_fact])
}

/// Encodes an accepted monotonic distillation checkpoint.
pub fn plan_memory_distillation_checkpoint(
    context: &MemoryMaintenanceContext,
    checkpoint_id: &str,
    watermark: &DistillationWatermark,
) -> Result<FactDraft, RuntimeCommandError> {
    validate_context(context)?;
    validate_text(checkpoint_id)?;
    if watermark.session_id != context.session_id.as_str() {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    fact(
        "memory.distillation_checkpointed",
        checkpoint_id,
        json!({"checkpoint_id": checkpoint_id, "namespace_id": context.namespace_id,
            "extractor_revision": watermark.extractor_revision, "session_id": watermark.session_id,
            "through_position": watermark.through_position, "batch_digest": watermark.batch_digest}),
        &context.recorded_at,
    )
}

/// Encodes only bounded audit digests and action count, never rejected content.
#[allow(clippy::too_many_arguments)]
pub fn plan_memory_audit(
    context: &MemoryMaintenanceContext,
    audit_id: &str,
    through_position: u64,
    policy_digest: &str,
    inventory_digest: &str,
    report_digest: &str,
    report: &MemoryAuditReport,
) -> Result<FactDraft, RuntimeCommandError> {
    validate_context(context)?;
    validate_text(audit_id)?;
    if through_position == 0 {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    for digest in [policy_digest, inventory_digest, report_digest] {
        validate_digest(digest)?;
    }
    fact(
        "memory.audit_recorded",
        audit_id,
        json!({"audit_id": audit_id, "namespace_id": context.namespace_id,
            "through_position": through_position, "policy_digest": policy_digest,
            "inventory_digest": inventory_digest, "report_digest": report_digest,
            "action_count": report.actions.len(), "truncated": report.truncated}),
        &context.recorded_at,
    )
}

/// Encodes an eligible promotion request before Knowledge publication.
pub fn plan_memory_promotion_request(
    context: &MemoryMaintenanceContext,
    request: &MemoryPromotionRequest,
) -> Result<FactDraft, RuntimeCommandError> {
    validate_context(context)?;
    if request.namespace_id() != context.namespace_id {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    fact(
        "memory.promotion_requested",
        request.request_id(),
        json!({"request_id": request.request_id(), "namespace_id": context.namespace_id,
            "record_id": request.record_id(), "revision_id": request.revision_id(),
            "memory_type": memory_type(request.memory_type()), "policy_revision": request.policy_revision(),
            "knowledge_proposal_id": request.knowledge_proposal_id(), "evidence_digest": request.evidence_digest()}),
        &context.recorded_at,
    )
}

/// Promotion receipt and lifecycle transition that must commit atomically.
pub struct PlannedMemoryPromotion {
    /// Receipt fact followed by its exact lifecycle transition.
    pub facts: Vec<FactDraft>,
    /// Promoted lifecycle made visible only after commit.
    pub lifecycle: MemoryLifecycle,
}

/// Verifies and encodes a Knowledge publication receipt plus Promoted transition.
pub fn plan_memory_promotion_receipt(
    context: &MemoryMaintenanceContext,
    request: &MemoryPromotionRequest,
    receipt: &MemoryPromotionReceipt,
    lifecycle: &MemoryLifecycle,
    position: u64,
) -> Result<PlannedMemoryPromotion, RuntimeCommandError> {
    validate_context(context)?;
    if request.namespace_id() != context.namespace_id {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let promoted = complete_memory_promotion(request, receipt, lifecycle, position)
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let receipt_fact = fact(
        "memory.promotion_recorded",
        request.request_id(),
        json!({"request_id": request.request_id(), "namespace_id": context.namespace_id,
            "record_id": request.record_id(), "revision_id": request.revision_id(),
            "knowledge_proposal_id": receipt.knowledge_proposal_id(),
            "knowledge_record_id": receipt.knowledge_record_id(),
            "knowledge_revision_id": receipt.knowledge_revision_id(), "receipt_digest": receipt.receipt_digest()}),
        &context.recorded_at,
    )?;
    let tally = promoted.tally();
    let lifecycle_fact = fact(
        "memory.lifecycle_transitioned",
        request.request_id(),
        json!({"transition_id": format!("transition-{}", request.request_id()),
            "namespace_id": context.namespace_id, "record_id": request.record_id(),
            "revision_id": request.revision_id(), "from_state": state(lifecycle.state()),
            "to_state": "promoted", "verified": tally.verified, "falsified": tally.falsified,
            "neutral": tally.neutral, "last_observed_position": position,
            "cause_kind": "promotion", "cause_id": request.request_id(),
            "promoted_knowledge_receipt_digest": receipt.receipt_digest()}),
        &context.recorded_at,
    )?;
    Ok(PlannedMemoryPromotion {
        facts: vec![receipt_fact, lifecycle_fact],
        lifecycle: promoted,
    })
}

/// Builds the erasure-request fact after validating the referenced tombstone.
pub fn plan_memory_forget(
    context: &MemoryMaintenanceContext,
    through_position: u64,
    tombstone: PlannedMemoryTombstone,
    request: &MemoryErasureRequest,
) -> Result<Vec<FactDraft>, RuntimeCommandError> {
    validate_context(context)?;
    let reference = request.tombstone_fact();
    let payload: Value = serde_json::from_str(tombstone.fact.payload.as_json())
        .map_err(|_| RuntimeCommandError::InvariantViolation)?;
    if request.namespace_id() != context.namespace_id
        || reference.session_id() != context.session_id.as_str()
        || reference.position()
            != through_position
                .checked_add(1)
                .ok_or(RuntimeCommandError::InvalidCommand)?
        || reference.fact_id() != tombstone.fact.fact_id.as_str()
        || reference.payload_digest() != tombstone.fact.payload.sha256()
        || payload["record_id"] != request.record_id()
        || payload["revision_id"] != request.revision_id()
    {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let request_fact = erasure_request_fact(context, request)?;
    Ok(vec![tombstone.fact, request_fact])
}

/// Encodes one complete-coverage physical erasure attempt.
pub fn plan_memory_erasure_receipt(
    context: &MemoryMaintenanceContext,
    request: &MemoryErasureRequest,
    receipt: &MemoryErasureReceipt,
) -> Result<FactDraft, RuntimeCommandError> {
    validate_context(context)?;
    if request.namespace_id() != context.namespace_id
        || receipt.request_id() != request.request_id()
    {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let results = receipt.results().iter().map(|result| {
        let mut value = json!({"target_id": result.target_id(), "status": target_status(result.status()),
            "receipt_digest": result.receipt_digest()});
        if let Some(position) = result.not_before_position() {
            value.as_object_mut().unwrap().insert("not_before_position".into(), json!(position));
        }
        value
    }).collect::<Vec<_>>();
    fact(
        "memory.erasure_recorded",
        receipt.attempt_id(),
        json!({"request_id": request.request_id(), "namespace_id": context.namespace_id,
            "record_id": request.record_id(), "revision_id": request.revision_id(),
            "attempt_id": receipt.attempt_id(), "attempted_at_position": receipt.attempted_at_position(),
            "results": results, "disposition": match receipt.disposition() { ErasureDisposition::Complete => "complete", ErasureDisposition::Partial => "partial" }}),
        &context.recorded_at,
    )
}

fn erasure_request_fact(
    context: &MemoryMaintenanceContext,
    request: &MemoryErasureRequest,
) -> Result<FactDraft, RuntimeCommandError> {
    let reference = request.tombstone_fact();
    let targets = request
        .targets()
        .iter()
        .map(|target| {
            json!({"target_id": target.target_id(),
        "kind": target_kind(target.kind())})
        })
        .collect::<Vec<_>>();
    fact(
        "memory.erasure_requested",
        request.request_id(),
        json!({"request_id": request.request_id(),
        "namespace_id": context.namespace_id, "record_id": request.record_id(), "revision_id": request.revision_id(),
        "tombstone_fact": {"session_id": reference.session_id(), "position": reference.position(),
            "fact_id": reference.fact_id(), "payload_digest": reference.payload_digest()},
        "policy_revision": request.policy_revision(), "targets": targets}),
        &context.recorded_at,
    )
}

fn fact(
    kind: &str,
    identity: &str,
    payload: Value,
    recorded_at: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("{kind}:{identity}").as_bytes())
    );
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{hash}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: recorded_at.into(),
    })
}

fn validate_context(value: &MemoryMaintenanceContext) -> Result<(), RuntimeCommandError> {
    validate_text(&value.namespace_id)?;
    if DateTime::parse_from_rfc3339(&value.recorded_at).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value.recorded_at
    }) {
        Ok(())
    } else {
        Err(RuntimeCommandError::InvalidCommand)
    }
}
fn validate_text(value: &str) -> Result<(), RuntimeCommandError> {
    if value.is_empty() || value.len() > 512 || value.trim() != value {
        Err(RuntimeCommandError::InvalidCommand)
    } else {
        Ok(())
    }
}
fn validate_digest(value: &str) -> Result<(), RuntimeCommandError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(RuntimeCommandError::InvalidCommand)
    }
}
fn source(value: MemoryCandidateSource) -> &'static str {
    match value {
        MemoryCandidateSource::ExplicitUserCommand => "explicit_user_command",
        MemoryCandidateSource::SessionEnd => "session_end",
        MemoryCandidateSource::ExitSummary => "exit_summary",
        MemoryCandidateSource::ScheduledDistillation => "scheduled_distillation",
    }
}
fn decision_kind(value: &MemoryMaintenanceDecision) -> &'static str {
    match value {
        MemoryMaintenanceDecision::Add { .. } => "add",
        MemoryMaintenanceDecision::Update { .. } => "update",
        MemoryMaintenanceDecision::Delete { .. } => "delete",
        MemoryMaintenanceDecision::Noop { .. } => "noop",
    }
}
fn memory_type(value: garive_memory::MemoryType) -> &'static str {
    match value {
        garive_memory::MemoryType::Semantic => "semantic",
        garive_memory::MemoryType::Episodic => "episodic",
        garive_memory::MemoryType::Lesson => "lesson",
        garive_memory::MemoryType::Procedural => "procedural",
    }
}
fn state(value: HypothesisState) -> &'static str {
    match value {
        HypothesisState::Candidate => "candidate",
        HypothesisState::Active => "active",
        HypothesisState::Cold => "cold",
        HypothesisState::Archived => "archived",
        HypothesisState::Promoted => "promoted",
    }
}
fn target_kind(value: ErasureTargetKind) -> &'static str {
    match value {
        ErasureTargetKind::PrimaryStore => "primary_store",
        ErasureTargetKind::Projection => "projection",
        ErasureTargetKind::Cache => "cache",
        ErasureTargetKind::Backup => "backup",
    }
}
fn target_status(value: ErasureTargetStatus) -> &'static str {
    match value {
        ErasureTargetStatus::Erased => "erased",
        ErasureTargetStatus::NotPresent => "not_present",
        ErasureTargetStatus::PendingBackupRetention => "pending_backup_retention",
        ErasureTargetStatus::PendingRetry => "pending_retry",
    }
}
