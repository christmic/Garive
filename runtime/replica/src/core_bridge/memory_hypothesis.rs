use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use garive_core::{
    FactRef, MemoryContextItem, MemoryContextState, MemoryRecallContextBatch, MemoryRecallProduct,
};
use garive_ledger::{
    CanonicalPayload, DurableFact, ExecutionId, FactDraft, FactId, FactKind, TurnId,
};
use garive_memory::{
    reduce_observation, select_recall, HypothesisState, MemoryAuthority, MemoryLifecycle,
    MemoryObligation, MemoryObservation, MemoryRecallCandidate, MemoryType,
    ObservationEvidenceKind, ObservationReduction, ObservationVerdict, RecallProduct,
    RecallSelectionKind, RecallSelectionRequest,
};
use serde_json::{json, Value};

use crate::RuntimeCommandError;

use super::encoding::digest;

/// Turn/Execution ownership for one commit-before-context M1 recall.
pub struct MemoryRecallContext {
    /// Stable selection identity.
    pub selection_id: String,
    /// Authorized opaque namespace.
    pub namespace_id: String,
    /// Digest over the complete semantic selection request.
    pub request_digest: String,
    /// Exact ledger prefix used by retrieval.
    pub through_position: u64,
    /// Consuming Turn.
    pub turn_id: TurnId,
    /// Consuming disposable Execution.
    pub execution_id: ExecutionId,
    /// Canonical Runtime observation time.
    pub recorded_at: String,
}

/// M1 recall result paired with the fact that must commit first.
pub struct PlannedMemoryRecall {
    /// Exact portable selection.
    pub selection: garive_memory::RecallSelection,
    /// Durable semantic request/result binding.
    pub fact: FactDraft,
}

/// Selects and encodes one exact replayable M1 recall.
pub fn plan_memory_recall(
    context: &MemoryRecallContext,
    candidates: &[MemoryRecallCandidate],
    request: &RecallSelectionRequest,
) -> Result<PlannedMemoryRecall, RuntimeCommandError> {
    validate_context(
        &context.selection_id,
        &context.namespace_id,
        &context.request_digest,
        &context.recorded_at,
    )?;
    let selection =
        select_recall(candidates, request).map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let items = selection.items.iter().map(|item| {
        let candidate = item.candidate();
        let score = candidate.score();
        let mut value = json!({
            "record_id": candidate.record_id(), "revision_id": candidate.revision_id(),
            "memory_type": memory_type(candidate.memory_type()), "role": role(candidate.role()),
            "authority": authority(candidate.authority()), "state": state(candidate.state()),
            "safe_label": candidate.safe_label(), "content_digest": candidate.content_digest(),
            "content_byte_length": candidate.content_bytes(), "evidence_count": candidate.evidence_count(),
            "relevance_basis_points": score.relevance, "recency_basis_points": score.recency,
            "importance_basis_points": score.importance,
            "selection_kind": match item.kind() { RecallSelectionKind::Ranked => "ranked", RecallSelectionKind::Explored => "explored" },
        });
        if let Some(draw) = item.draw_hex() { value.as_object_mut().unwrap().insert("draw_hex".into(), json!(draw)); }
        value
    }).collect::<Vec<_>>();
    let mut payload = json!({
        "selection_id": context.selection_id, "request_digest": context.request_digest,
        "namespace_id": context.namespace_id,
        "product": match request.product() { RecallProduct::Menu => "menu", RecallProduct::Detail => "detail" },
        "selection_policy_revision": request.selection_policy_revision(), "through_position": context.through_position,
        "max_items": request.max_items(), "max_total_bytes": request.max_total_bytes(),
        "items": items, "truncated": selection.truncated,
    });
    if let Some(exploration) = request.exploration() {
        payload.as_object_mut().unwrap().insert("exploration".into(), json!({
            "algorithm_revision": exploration.algorithm_revision(), "seed": exploration.seed(), "slots": exploration.slots(),
        }));
    }
    let fact = fact(
        "memory.recall_recorded",
        &context.selection_id,
        Some((&context.turn_id, &context.execution_id)),
        payload,
        &context.recorded_at,
    )?;
    Ok(PlannedMemoryRecall { selection, fact })
}

/// Decodes an actual committed recall fact into the provider-neutral C2 adapter value.
pub fn decode_committed_memory_recall(
    fact: &DurableFact,
    resolved_detail: &BTreeMap<(String, String), String>,
) -> Result<MemoryRecallContextBatch, RuntimeCommandError> {
    if fact.kind.as_str() != "memory.recall_recorded" {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let value: Value = serde_json::from_str(fact.payload.as_json())
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let object = value
        .as_object()
        .ok_or(RuntimeCommandError::InvalidCommand)?;
    let product = match text(object, "product")?.as_str() {
        "menu" if resolved_detail.is_empty() => MemoryRecallProduct::Menu,
        "detail" => MemoryRecallProduct::Detail,
        _ => return Err(RuntimeCommandError::InvalidCommand),
    };
    let values = object
        .get("items")
        .and_then(Value::as_array)
        .ok_or(RuntimeCommandError::InvalidCommand)?;
    if product == MemoryRecallProduct::Detail && values.len() != resolved_detail.len() {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let items = values
        .iter()
        .map(|value| decode_context_item(value, product, resolved_detail))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MemoryRecallContextBatch {
        fact_ref: FactRef {
            session_id: fact.session_id.as_str().into(),
            position: fact.position,
        },
        fact_id: fact.fact_id.as_str().into(),
        payload_digest: fact.payload.sha256().into(),
        selection_id: text(object, "selection_id")?,
        request_digest: text(object, "request_digest")?,
        namespace_id: text(object, "namespace_id")?,
        product,
        selection_policy_revision: text(object, "selection_policy_revision")?,
        through_position: number(object, "through_position")?,
        truncated: object
            .get("truncated")
            .and_then(Value::as_bool)
            .ok_or(RuntimeCommandError::InvalidCommand)?,
        items,
    })
}

fn decode_context_item(
    value: &Value,
    product: MemoryRecallProduct,
    resolved: &BTreeMap<(String, String), String>,
) -> Result<MemoryContextItem, RuntimeCommandError> {
    let object = value
        .as_object()
        .ok_or(RuntimeCommandError::InvalidCommand)?;
    let record_id = text(object, "record_id")?;
    let revision_id = text(object, "revision_id")?;
    let content_utf8 = match product {
        MemoryRecallProduct::Menu => None,
        MemoryRecallProduct::Detail => Some(
            resolved
                .get(&(record_id.clone(), revision_id.clone()))
                .ok_or(RuntimeCommandError::InvalidCommand)?
                .clone(),
        ),
    };
    Ok(MemoryContextItem {
        record_id,
        revision_id,
        memory_type: text(object, "memory_type")?,
        role: text(object, "role")?,
        authority: text(object, "authority")?,
        state: match text(object, "state")?.as_str() {
            "candidate" => MemoryContextState::Candidate,
            "active" => MemoryContextState::Active,
            "cold" => MemoryContextState::Cold,
            "archived" => MemoryContextState::Archived,
            _ => return Err(RuntimeCommandError::InvalidCommand),
        },
        safe_label: text(object, "safe_label")?,
        content_digest: text(object, "content_digest")?,
        content_byte_length: number(object, "content_byte_length")?,
        content_utf8,
    })
}

fn text(
    value: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, RuntimeCommandError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or(RuntimeCommandError::InvalidCommand)
}

fn number(value: &serde_json::Map<String, Value>, field: &str) -> Result<u64, RuntimeCommandError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .filter(|number| *number != 0)
        .ok_or(RuntimeCommandError::InvalidCommand)
}

/// Turn/Execution and namespace ownership for opening an obligation.
pub struct MemoryObligationContext {
    /// Authorized opaque namespace.
    pub namespace_id: String,
    /// Applying Turn.
    pub turn_id: TurnId,
    /// Applying Execution.
    pub execution_id: ExecutionId,
    /// Canonical Runtime observation time.
    pub recorded_at: String,
}

/// Encodes an application claim as an open durable obligation.
pub fn plan_memory_obligation(
    context: &MemoryObligationContext,
    obligation: &MemoryObligation,
) -> Result<FactDraft, RuntimeCommandError> {
    validate_text(&context.namespace_id)?;
    validate_time(&context.recorded_at)?;
    let application = obligation.application_fact();
    fact(
        "memory.obligation_opened",
        obligation.obligation_id(),
        Some((&context.turn_id, &context.execution_id)),
        json!({
            "obligation_id": obligation.obligation_id(), "namespace_id": context.namespace_id,
            "record_id": obligation.record_id(), "revision_id": obligation.revision_id(),
            "application_fact": {"session_id": application.session_id(), "position": application.position(),
                "fact_id": application.fact_id(), "payload_digest": application.payload_digest()},
            "expected_outcome_digest": obligation.expected_outcome_digest(),
            "application_scope_digest": obligation.application_scope_digest(),
            "attribution_policy_revision": obligation.attribution_policy_revision(),
            "expires_at_position": obligation.expires_at_position(),
        }),
        &context.recorded_at,
    )
}

/// Session-scoped ownership for an asynchronous observation batch.
pub struct MemoryObservationContext {
    /// Authorized opaque namespace.
    pub namespace_id: String,
    /// Canonical Runtime observation time.
    pub recorded_at: String,
}

/// Observation and lifecycle facts that must commit atomically.
pub struct PlannedMemoryObservation {
    /// Observation followed by its exact lifecycle transition.
    pub facts: Vec<FactDraft>,
    /// Pure portable reduction used to build the facts.
    pub reduction: ObservationReduction,
}

/// Reconciles reality evidence and plans an atomic session-scoped fact pair.
pub fn plan_memory_observation(
    context: &MemoryObservationContext,
    obligation: &MemoryObligation,
    observation: &MemoryObservation,
    lifecycle: &MemoryLifecycle,
) -> Result<PlannedMemoryObservation, RuntimeCommandError> {
    validate_text(&context.namespace_id)?;
    validate_time(&context.recorded_at)?;
    let reduction = reduce_observation(obligation, observation, lifecycle)
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let evidence = observation.evidence().iter().map(|item| {
        let fact = item.fact();
        json!({"kind": evidence_kind(item.kind()), "fact": {"session_id": fact.session_id(),
            "position": fact.position(), "fact_id": fact.fact_id(), "payload_digest": fact.payload_digest()}})
    }).collect::<Vec<_>>();
    let verdict = match observation.verdict() {
        ObservationVerdict::Verified => json!({"kind": "verified"}),
        ObservationVerdict::Neutral { safe_reason } => {
            json!({"kind": "neutral", "safe_reason": safe_reason})
        }
        ObservationVerdict::Falsified {
            in_scope,
            observed_scope_digest,
        } => {
            let mut value = json!({"kind": "falsified", "in_scope": in_scope});
            if let Some(scope) = observed_scope_digest {
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("observed_scope_digest".into(), json!(scope));
            }
            value
        }
    };
    let observation_fact = fact(
        "memory.observation_recorded",
        observation.observation_id(),
        None,
        json!({
            "observation_id": observation.observation_id(), "obligation_id": observation.obligation_id(),
            "namespace_id": context.namespace_id, "position": observation.position(),
            "verifier_revision": observation.verifier_revision(), "evidence": evidence, "verdict": verdict,
        }),
        &context.recorded_at,
    )?;
    let mut lifecycle_payload = json!({
        "transition_id": format!("transition-{}", observation.observation_id()),
        "namespace_id": context.namespace_id, "record_id": obligation.record_id(), "revision_id": obligation.revision_id(),
        "from_state": state(lifecycle.state()), "to_state": state(reduction.lifecycle.state()),
        "verified": reduction.lifecycle.tally().verified, "falsified": reduction.lifecycle.tally().falsified,
        "neutral": reduction.lifecycle.tally().neutral, "last_observed_position": reduction.lifecycle.last_observed_position(),
        "cause_kind": "observation", "cause_id": observation.observation_id(),
    });
    if let Some(receipt) = reduction.lifecycle.promoted_knowledge_receipt_digest() {
        lifecycle_payload
            .as_object_mut()
            .unwrap()
            .insert("promoted_knowledge_receipt_digest".into(), json!(receipt));
    }
    let lifecycle_fact = fact(
        "memory.lifecycle_transitioned",
        observation.observation_id(),
        None,
        lifecycle_payload,
        &context.recorded_at,
    )?;
    Ok(PlannedMemoryObservation {
        facts: vec![observation_fact, lifecycle_fact],
        reduction,
    })
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

fn validate_context(
    identity: &str,
    namespace: &str,
    request_digest: &str,
    time: &str,
) -> Result<(), RuntimeCommandError> {
    validate_text(identity)?;
    validate_text(namespace)?;
    if request_digest.len() != 64
        || !request_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    validate_time(time)
}
fn validate_text(value: &str) -> Result<(), RuntimeCommandError> {
    if value.is_empty() || value.trim() != value {
        Err(RuntimeCommandError::InvalidCommand)
    } else {
        Ok(())
    }
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
const fn memory_type(value: MemoryType) -> &'static str {
    match value {
        MemoryType::Semantic => "semantic",
        MemoryType::Episodic => "episodic",
        MemoryType::Lesson => "lesson",
        MemoryType::Procedural => "procedural",
    }
}
const fn role(value: garive_memory::MemoryKind) -> &'static str {
    match value {
        garive_memory::MemoryKind::Preference => "preference",
        garive_memory::MemoryKind::Constraint => "constraint",
        garive_memory::MemoryKind::Decision => "decision",
        garive_memory::MemoryKind::LearnedFact => "learned_fact",
        garive_memory::MemoryKind::Summary => "summary",
    }
}
const fn authority(value: MemoryAuthority) -> &'static str {
    match value {
        MemoryAuthority::UserDeclared => "user_declared",
        MemoryAuthority::AgentLearned => "agent_learned",
        MemoryAuthority::OrganisationPublished => "organisation_published",
    }
}
const fn state(value: HypothesisState) -> &'static str {
    match value {
        HypothesisState::Candidate => "candidate",
        HypothesisState::Active => "active",
        HypothesisState::Cold => "cold",
        HypothesisState::Archived => "archived",
        HypothesisState::Promoted => "promoted",
    }
}
const fn evidence_kind(value: ObservationEvidenceKind) -> &'static str {
    match value {
        ObservationEvidenceKind::ToolResult => "tool_result",
        ObservationEvidenceKind::TestResult => "test_result",
        ObservationEvidenceKind::EffectReceipt => "effect_receipt",
        ObservationEvidenceKind::UserCorrection => "user_correction",
        ObservationEvidenceKind::DeterministicVerifier => "deterministic_verifier",
    }
}
