use chrono::{DateTime, SecondsFormat, Utc};
use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, SessionId, TurnId,
};
use garive_memory::{
    MemoryAuthority, MemoryCommit, MemoryErrorCode, MemoryProposal, MemoryRecord,
    MemoryRevisionClassification, MemoryScope, MemoryScopeClass, MemoryState, MemoryTombstone,
    MemoryType,
};
use serde_json::{json, Value};

use crate::RuntimeCommandError;

use super::encoding::digest;

/// Durable ownership and predicted positions for one direct M0 write decision.
pub struct MemoryWriteContext {
    /// Turn that owns the proposal and decision.
    pub turn_id: TurnId,
    /// Execution that owns the proposal and decision.
    pub execution_id: ExecutionId,
    /// Position immediately before the atomic decision batch.
    pub through_position: u64,
    /// Canonical RFC 3339 UTC observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Stable direct rejection codes admitted by `memory.rejected.v1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryWriteRejection {
    /// Runtime denied the namespace or scope.
    NamespaceDenied,
    /// Referenced evidence does not exist.
    EvidenceNotFound,
    /// Referenced evidence did not match its binding.
    EvidenceMismatch,
    /// Optimistic active revision conflicted.
    RevisionConflict,
    /// Retention policy denied the write.
    RetentionRejected,
    /// Sensitivity policy denied the write.
    SensitivityDenied,
    /// A configured bound was exceeded.
    LimitExceeded,
    /// The operation is not admitted.
    Unsupported,
}

/// Frozen Runtime authority result for a direct proposal.
pub enum MemoryWriteDecision {
    /// Commit one exact immutable revision.
    Commit(MemoryCommit),
    /// Reject with one stable admitted reason.
    Reject(MemoryWriteRejection),
}

/// Pure direct-write reduction paired with its atomic durable fact batch.
pub struct PlannedMemoryWrite {
    /// Facts ordered as proposal, decision, then optional supersession.
    pub facts: Vec<FactDraft>,
    /// State after the accepted batch; unchanged for a rejection.
    pub next_state: MemoryState,
    /// Newly committed record, absent for a rejection.
    pub record: Option<MemoryRecord>,
}

/// Plans an atomic proposal/decision batch without persistence or authority discovery.
pub fn plan_memory_write(
    context: &MemoryWriteContext,
    state: &MemoryState,
    proposal: &MemoryProposal,
    decision: MemoryWriteDecision,
) -> Result<PlannedMemoryWrite, RuntimeCommandError> {
    validate_time(&context.recorded_at)?;
    let proposed = fact(
        "memory.proposed",
        proposal.proposal_id(),
        Some((&context.turn_id, &context.execution_id)),
        serde_json::to_value(proposal).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        &context.recorded_at,
    )?;
    match decision {
        MemoryWriteDecision::Reject(reason) => Ok(PlannedMemoryWrite {
            facts: vec![
                proposed,
                fact(
                    "memory.rejected",
                    proposal.proposal_id(),
                    Some((&context.turn_id, &context.execution_id)),
                    json!({"proposal_id": proposal.proposal_id(), "reason": rejection(reason)}),
                    &context.recorded_at,
                )?,
            ],
            next_state: state.clone(),
            record: None,
        }),
        MemoryWriteDecision::Commit(commit) => {
            let expected_position = context
                .through_position
                .checked_add(2)
                .ok_or(RuntimeCommandError::InvalidCommand)?;
            if commit.valid_from_position() != expected_position {
                return Err(RuntimeCommandError::InvalidCommand);
            }
            let mut next_state = state.clone();
            let outcome = next_state
                .commit(proposal, &commit)
                .map_err(map_memory_error)?;
            let mut payload = serde_json::to_value(&outcome.record)
                .map_err(|_| RuntimeCommandError::InvariantViolation)?
                .as_object()
                .cloned()
                .ok_or(RuntimeCommandError::InvariantViolation)?;
            payload.remove("status");
            payload.insert("proposal_id".into(), json!(proposal.proposal_id()));
            payload.insert(
                "retention_policy_digest".into(),
                json!(commit.retention_policy_digest()),
            );
            let mut facts = vec![
                proposed,
                fact(
                    "memory.committed",
                    proposal.proposal_id(),
                    Some((&context.turn_id, &context.execution_id)),
                    Value::Object(payload),
                    &context.recorded_at,
                )?,
            ];
            if let Some(binding) = &outcome.supersession {
                facts.push(fact(
                    "memory.superseded",
                    proposal.proposal_id(),
                    Some((&context.turn_id, &context.execution_id)),
                    json!({
                        "record_id": binding.record_id,
                        "old_revision_id": binding.old_revision_id,
                        "new_revision_id": binding.new_revision_id,
                        "proposal_id": binding.proposal_id,
                    }),
                    &context.recorded_at,
                )?);
            }
            Ok(PlannedMemoryWrite {
                facts,
                next_state,
                record: Some(outcome.record),
            })
        }
    }
}

/// Plans an accepted M0 write and its registry-admitted M1 classification as one fact batch.
#[allow(clippy::too_many_arguments)]
pub fn plan_classified_memory_write(
    context: &MemoryWriteContext,
    state: &MemoryState,
    proposal: &MemoryProposal,
    commit: MemoryCommit,
    session_id: &SessionId,
    classification_id: &str,
    classification: &MemoryRevisionClassification,
) -> Result<PlannedMemoryWrite, RuntimeCommandError> {
    if classification.role() != proposal.kind()
        || classification_id.is_empty()
        || !classification_scope_matches(proposal.scope(), classification)
    {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let mut planned = plan_memory_write(
        context,
        state,
        proposal,
        MemoryWriteDecision::Commit(commit),
    )?;
    let committed = planned
        .facts
        .get(1)
        .filter(|fact| fact.kind.as_str() == "memory.committed")
        .ok_or(RuntimeCommandError::InvariantViolation)?;
    let position = context
        .through_position
        .checked_add(2)
        .ok_or(RuntimeCommandError::InvalidCommand)?;
    let record = planned
        .record
        .as_ref()
        .ok_or(RuntimeCommandError::InvariantViolation)?;
    let mut payload = json!({
        "classification_id": classification_id,
        "namespace_id": proposal.namespace_id(),
        "record_id": record.record_id(),
        "revision_id": record.revision_id(),
        "memory_type": memory_type(classification.memory_type()),
        "authority": memory_authority(classification.authority().authority()),
        "lifecycle": initial_lifecycle(classification.lifecycle())?,
        "scope": memory_scope_class(classification.scope().scope()),
        "scope_owner_id": classification.scope().owner_id(),
        "policy_revision": classification.policy_revision(),
        "source_commit": {
            "session_id": session_id.as_str(),
            "position": position,
            "fact_id": committed.fact_id.as_str(),
            "payload_digest": committed.payload.sha256(),
        },
    });
    if let Some(receipt) = classification.authority().receipt_digest() {
        payload
            .as_object_mut()
            .ok_or(RuntimeCommandError::InvariantViolation)?
            .insert("authority_receipt_digest".into(), json!(receipt));
    }
    if let Some(policy) = classification.scope().aggregation_policy_digest() {
        payload
            .as_object_mut()
            .ok_or(RuntimeCommandError::InvariantViolation)?
            .insert("aggregation_policy_digest".into(), json!(policy));
    }
    planned.facts.insert(
        2,
        fact(
            "memory.revision_classified",
            classification_id,
            None,
            payload,
            &context.recorded_at,
        )?,
    );
    Ok(planned)
}

fn classification_scope_matches(
    source: &MemoryScope,
    classification: &MemoryRevisionClassification,
) -> bool {
    let target = classification.scope();
    match source {
        MemoryScope::Session { owner_id } => {
            target.scope() == MemoryScopeClass::Session && target.owner_id() == owner_id
        }
        MemoryScope::AgentInstance { owner_id } => {
            target.scope() == MemoryScopeClass::AgentInstance && target.owner_id() == owner_id
        }
        MemoryScope::Namespace => matches!(
            target.scope(),
            MemoryScopeClass::User | MemoryScopeClass::Project | MemoryScopeClass::Platform
        ),
    }
}

/// Durable ownership for one session-scoped tombstone command.
pub struct MemoryTombstoneContext {
    /// Idempotency identity for the user/operator/policy command.
    pub command_id: String,
    /// Canonical RFC 3339 UTC observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Stable safe tombstone reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryTombstoneReason {
    /// Retention expiry removed the revision from future retrieval.
    Expired,
    /// A later revision replaced the target.
    Superseded,
    /// The authorized user requested removal.
    UserRequest,
    /// Runtime policy requires removal.
    Policy,
    /// Source integrity could no longer be established.
    CorruptSource,
}

/// Pure tombstone reduction paired with its session-scoped durable fact.
pub struct PlannedMemoryTombstone {
    /// Fact that must commit before the state becomes visible.
    pub fact: FactDraft,
    /// State after applying the exact active-revision tombstone.
    pub next_state: MemoryState,
}

/// Plans one exact active-revision tombstone without persistence.
pub fn plan_memory_tombstone(
    context: &MemoryTombstoneContext,
    state: &MemoryState,
    target: &MemoryTombstone,
    reason: MemoryTombstoneReason,
) -> Result<PlannedMemoryTombstone, RuntimeCommandError> {
    if reason == MemoryTombstoneReason::UserRequest {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    plan_memory_tombstone_inner(context, state, target, reason)
}

pub(super) fn plan_user_memory_tombstone(
    context: &MemoryTombstoneContext,
    state: &MemoryState,
    target: &MemoryTombstone,
) -> Result<PlannedMemoryTombstone, RuntimeCommandError> {
    plan_memory_tombstone_inner(context, state, target, MemoryTombstoneReason::UserRequest)
}

fn plan_memory_tombstone_inner(
    context: &MemoryTombstoneContext,
    state: &MemoryState,
    target: &MemoryTombstone,
    reason: MemoryTombstoneReason,
) -> Result<PlannedMemoryTombstone, RuntimeCommandError> {
    validate_time(&context.recorded_at)?;
    if context.command_id.is_empty() {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let mut next_state = state.clone();
    next_state.tombstone(target).map_err(map_memory_error)?;
    let namespace_id = next_state
        .revisions()
        .iter()
        .find(|record| {
            record.record_id() == target.record_id() && record.revision_id() == target.revision_id()
        })
        .map(|record| record.namespace_id().to_owned())
        .ok_or(RuntimeCommandError::InvariantViolation)?;
    let fact = fact(
        "memory.tombstoned",
        &context.command_id,
        None,
        json!({
            "command_id": context.command_id,
            "namespace_id": namespace_id,
            "record_id": target.record_id(),
            "revision_id": target.revision_id(),
            "reason": tombstone_reason(reason),
        }),
        &context.recorded_at,
    )?;
    Ok(PlannedMemoryTombstone { fact, next_state })
}

fn fact(
    kind: &str,
    identity: &str,
    owner: Option<(&TurnId, &ExecutionId)>,
    payload: Value,
    recorded_at: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    let fact_digest = digest(format!("{kind}:{identity}").as_bytes());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{fact_digest}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: owner.map(|value| value.0.clone()),
        execution_id: owner.map(|value| value.1.clone()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: recorded_at.into(),
    })
}

fn validate_time(value: &str) -> Result<(), RuntimeCommandError> {
    if DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    }) {
        Ok(())
    } else {
        Err(RuntimeCommandError::InvalidCommand)
    }
}

const fn rejection(value: MemoryWriteRejection) -> &'static str {
    match value {
        MemoryWriteRejection::NamespaceDenied => "namespace_denied",
        MemoryWriteRejection::EvidenceNotFound => "evidence_not_found",
        MemoryWriteRejection::EvidenceMismatch => "evidence_mismatch",
        MemoryWriteRejection::RevisionConflict => "revision_conflict",
        MemoryWriteRejection::RetentionRejected => "retention_rejected",
        MemoryWriteRejection::SensitivityDenied => "sensitivity_denied",
        MemoryWriteRejection::LimitExceeded => "limit_exceeded",
        MemoryWriteRejection::Unsupported => "unsupported",
    }
}

const fn tombstone_reason(value: MemoryTombstoneReason) -> &'static str {
    match value {
        MemoryTombstoneReason::Expired => "expired",
        MemoryTombstoneReason::Superseded => "superseded",
        MemoryTombstoneReason::UserRequest => "user_request",
        MemoryTombstoneReason::Policy => "policy",
        MemoryTombstoneReason::CorruptSource => "corrupt_source",
    }
}

fn map_memory_error(error: garive_memory::MemoryError) -> RuntimeCommandError {
    match error.code() {
        MemoryErrorCode::RevisionConflict => RuntimeCommandError::CommandConflict,
        _ => RuntimeCommandError::InvalidCommand,
    }
}

const fn memory_type(value: MemoryType) -> &'static str {
    match value {
        MemoryType::Semantic => "semantic",
        MemoryType::Episodic => "episodic",
        MemoryType::Lesson => "lesson",
        MemoryType::Procedural => "procedural",
    }
}

const fn memory_authority(value: MemoryAuthority) -> &'static str {
    match value {
        MemoryAuthority::UserDeclared => "user_declared",
        MemoryAuthority::AgentLearned => "agent_learned",
        MemoryAuthority::OrganisationPublished => "organisation_published",
    }
}

const fn memory_scope_class(value: MemoryScopeClass) -> &'static str {
    match value {
        MemoryScopeClass::Session => "session",
        MemoryScopeClass::AgentInstance => "agent_instance",
        MemoryScopeClass::User => "user",
        MemoryScopeClass::Project => "project",
        MemoryScopeClass::Platform => "platform",
    }
}

fn initial_lifecycle(
    value: garive_memory::HypothesisState,
) -> Result<&'static str, RuntimeCommandError> {
    match value {
        garive_memory::HypothesisState::Candidate => Ok("candidate"),
        garive_memory::HypothesisState::Active => Ok("active"),
        _ => Err(RuntimeCommandError::InvalidCommand),
    }
}
