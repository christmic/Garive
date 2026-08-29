use chrono::{DateTime, SecondsFormat, Utc};
use garive_knowledge::{
    complete_knowledge, FreshnessRequirement, KnowledgeCompleted, KnowledgeEvidence,
    KnowledgeRequest, KnowledgeSourceDescriptor,
};
use garive_ledger::{CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, TurnId};
use serde_json::{json, Map, Value};

use crate::RuntimeCommandError;

use super::encoding::digest;

/// Durable ownership and observation time for one Knowledge lifecycle.
pub struct KnowledgeLifecycleContext {
    /// Turn that requested the evidence.
    pub turn_id: TurnId,
    /// Execution that may consume the evidence.
    pub execution_id: ExecutionId,
    /// Canonical RFC 3339 UTC observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Exact request binding and its redispatchable durable fact.
pub struct PreparedKnowledgeRequest {
    /// Logical Knowledge request identity.
    pub request_id: String,
    /// Canonical portable request digest.
    pub request_digest: String,
    /// Fact committed while connector dispatch remains safe.
    pub fact: FactDraft,
}

/// Validated bounded completion and its commit-before-model fact.
pub struct PlannedKnowledgeCompletion {
    /// Exact normalized evidence supplied to Core after commit.
    pub completed: KnowledgeCompleted,
    /// Terminal fact that must commit before model use.
    pub fact: FactDraft,
}

/// Stable phase for a terminal Knowledge failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeFailurePhase {
    /// Rejected before the connector boundary.
    PreDispatch,
    /// Dispatch may have crossed the connector boundary.
    Dispatched,
    /// A response arrived but failed exact validation.
    ResponseValidation,
}

/// Stable L0 reason for a terminal Knowledge failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeFailureReason {
    /// The portable query is invalid.
    InvalidQuery,
    /// The configured source does not exist.
    SourceNotFound,
    /// The exact configured source revision differs.
    SourceRevisionMismatch,
    /// Runtime authority denied the source.
    SourceDenied,
    /// A requested filter is unsupported.
    FilterUnsupported,
    /// The requested freshness cannot be supplied.
    FreshnessUnavailable,
    /// The connector is temporarily unavailable.
    Unavailable,
    /// The connector rejected the request.
    Rejected,
    /// Dispatch occurred without a trustworthy terminal result.
    Uncertain,
    /// Citation validation failed.
    CitationInvalid,
    /// Evidence and citation content digests differ.
    ContentDigestMismatch,
    /// The connector response violated a committed bound.
    LimitExceeded,
    /// A required durable write failed.
    DurabilityFailure,
    /// Persisted lifecycle state is impossible.
    CorruptKnowledgeState,
}

/// Plans the exact `knowledge.requested` redispatchable boundary.
pub fn plan_knowledge_requested(
    context: &KnowledgeLifecycleContext,
    source: &KnowledgeSourceDescriptor,
    request: &KnowledgeRequest,
) -> Result<PreparedKnowledgeRequest, RuntimeCommandError> {
    validate_context(context)?;
    request.validate_source(source).map_err(command)?;
    let request_digest = request.request_digest().map_err(command)?;
    let mut payload = Map::from_iter([
        ("request_id".into(), json!(request.request_id())),
        ("source_id".into(), json!(request.source_id())),
        ("source_revision".into(), json!(request.source_revision())),
        ("request_digest".into(), json!(request_digest)),
        (
            "mode".into(),
            serde_json::to_value(request.mode()).map_err(invariant)?,
        ),
        (
            "query".into(),
            serde_json::to_value(request.query()).map_err(invariant)?,
        ),
        (
            "filters".into(),
            serde_json::to_value(request.filters_binding().map_err(command)?).map_err(invariant)?,
        ),
        ("through_position".into(), json!(request.through_position())),
        ("max_chunks".into(), json!(request.max_chunks())),
        ("max_total_bytes".into(), json!(request.max_total_bytes())),
        (
            "deadline_budget_ms".into(),
            json!(request.deadline_budget_ms()),
        ),
    ]);
    match request.freshness_requirement() {
        FreshnessRequirement::CachedAllowed => {
            payload.insert("freshness_kind".into(), json!("cached_allowed"));
        }
        FreshnessRequirement::Revalidate => {
            payload.insert("freshness_kind".into(), json!("revalidate"));
        }
        FreshnessRequirement::ExactSnapshot { snapshot_digest } => {
            payload.insert("freshness_kind".into(), json!("exact_snapshot"));
            payload.insert("exact_snapshot_digest".into(), json!(snapshot_digest));
        }
    }
    let fact = fact(
        context,
        request.request_id(),
        "knowledge.requested",
        Value::Object(payload),
        None,
    )?;
    Ok(PreparedKnowledgeRequest {
        request_id: request.request_id().into(),
        request_digest,
        fact,
    })
}

/// Plans the durable uncertainty boundary immediately before connector dispatch.
pub fn plan_knowledge_dispatched(
    context: &KnowledgeLifecycleContext,
    prepared: &PreparedKnowledgeRequest,
    dispatch_attempt_id: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    if dispatch_attempt_id.is_empty() {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    fact(
        context,
        &prepared.request_id,
        "knowledge.dispatched",
        json!({"request_id":prepared.request_id,"request_digest":prepared.request_digest,"dispatch_attempt_id":dispatch_attempt_id}),
        Some(dispatch_attempt_id),
    )
}

/// Validates and plans one exact commit-before-model completion.
#[allow(clippy::too_many_arguments)]
pub fn plan_knowledge_completed(
    context: &KnowledgeLifecycleContext,
    prepared: &PreparedKnowledgeRequest,
    source: &KnowledgeSourceDescriptor,
    request: &KnowledgeRequest,
    evidence: Vec<KnowledgeEvidence>,
    connector_order_stable: bool,
) -> Result<PlannedKnowledgeCompletion, RuntimeCommandError> {
    let completed =
        complete_knowledge(request, source, evidence, connector_order_stable).map_err(command)?;
    let bindings = completed
        .evidence
        .iter()
        .map(evidence_value)
        .collect::<Result<Vec<_>, _>>()?;
    let fact = fact(
        context,
        &prepared.request_id,
        "knowledge.completed",
        json!({"request_id":prepared.request_id,"request_digest":prepared.request_digest,"evidence":bindings,"truncated":completed.truncated}),
        None,
    )?;
    Ok(PlannedKnowledgeCompletion { completed, fact })
}

/// Plans one exact terminal failure, including dispatched ambiguity semantics.
pub fn plan_knowledge_failed(
    context: &KnowledgeLifecycleContext,
    prepared: &PreparedKnowledgeRequest,
    phase: KnowledgeFailurePhase,
    reason: KnowledgeFailureReason,
    retry_after_ms: Option<u64>,
) -> Result<FactDraft, RuntimeCommandError> {
    if retry_after_ms == Some(0)
        || (phase == KnowledgeFailurePhase::Dispatched)
            != (reason == KnowledgeFailureReason::Uncertain)
    {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    let mut payload = Map::from_iter([
        ("request_id".into(), json!(prepared.request_id)),
        ("request_digest".into(), json!(prepared.request_digest)),
        ("phase".into(), json!(phase_name(phase))),
        ("reason".into(), json!(reason_name(reason))),
        (
            "ambiguous".into(),
            json!(phase == KnowledgeFailurePhase::Dispatched),
        ),
    ]);
    if let Some(delay) = retry_after_ms {
        payload.insert("retry_after_ms".into(), json!(delay));
    }
    fact(
        context,
        &prepared.request_id,
        "knowledge.failed",
        Value::Object(payload),
        None,
    )
}

fn evidence_value(value: &KnowledgeEvidence) -> Result<Value, RuntimeCommandError> {
    let mut binding = Map::from_iter([
        ("evidence_id".into(), json!(value.evidence_id())),
        (
            "content".into(),
            serde_json::to_value(value.content()).map_err(invariant)?,
        ),
        (
            "content_byte_length".into(),
            json!(value.content_byte_length()),
        ),
        (
            "citation_kind".into(),
            serde_json::to_value(value.citation().locator_kind()).map_err(invariant)?,
        ),
        ("citation_locator".into(), json!(value.citation().locator())),
        (
            "citation_content_digest".into(),
            json!(value.citation().content_digest()),
        ),
        ("retrieved_at_utc".into(), json!(value.retrieved_at_utc())),
        (
            "freshness".into(),
            serde_json::to_value(value.freshness()).map_err(invariant)?,
        ),
        (
            "trust_class".into(),
            serde_json::to_value(value.trust_class()).map_err(invariant)?,
        ),
        ("rank_basis_points".into(), json!(value.rank_basis_points())),
    ]);
    for (key, optional) in [
        ("source_snapshot_digest", value.source_snapshot_digest()),
        ("citation_title", value.citation().title()),
        ("canonical_uri", value.citation().canonical_uri()),
    ] {
        if let Some(optional) = optional {
            binding.insert(key.into(), json!(optional));
        }
    }
    Ok(Value::Object(binding))
}

fn fact(
    context: &KnowledgeLifecycleContext,
    request_id: &str,
    kind: &str,
    payload: Value,
    attempt: Option<&str>,
) -> Result<FactDraft, RuntimeCommandError> {
    validate_context(context)?;
    let id = digest(format!("{request_id}:{kind}:{}", attempt.unwrap_or("")).as_bytes());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{id}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: Some(context.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn validate_context(value: &KnowledgeLifecycleContext) -> Result<(), RuntimeCommandError> {
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

const fn phase_name(value: KnowledgeFailurePhase) -> &'static str {
    match value {
        KnowledgeFailurePhase::PreDispatch => "pre_dispatch",
        KnowledgeFailurePhase::Dispatched => "dispatched",
        KnowledgeFailurePhase::ResponseValidation => "response_validation",
    }
}

const fn reason_name(value: KnowledgeFailureReason) -> &'static str {
    match value {
        KnowledgeFailureReason::SourceDenied => "source_denied",
        KnowledgeFailureReason::InvalidQuery => "invalid_query",
        KnowledgeFailureReason::SourceNotFound => "source_not_found",
        KnowledgeFailureReason::SourceRevisionMismatch => "source_revision_mismatch",
        KnowledgeFailureReason::FilterUnsupported => "filter_unsupported",
        KnowledgeFailureReason::FreshnessUnavailable => "freshness_unavailable",
        KnowledgeFailureReason::Unavailable => "connector_unavailable",
        KnowledgeFailureReason::Rejected => "connector_rejected",
        KnowledgeFailureReason::Uncertain => "retrieval_uncertain",
        KnowledgeFailureReason::CitationInvalid => "citation_invalid",
        KnowledgeFailureReason::ContentDigestMismatch => "content_digest_mismatch",
        KnowledgeFailureReason::LimitExceeded => "limit_exceeded",
        KnowledgeFailureReason::DurabilityFailure => "durability_failure",
        KnowledgeFailureReason::CorruptKnowledgeState => "corrupt_knowledge_state",
    }
}

fn invariant(_: serde_json::Error) -> RuntimeCommandError {
    RuntimeCommandError::InvariantViolation
}

fn command(_: garive_knowledge::KnowledgeError) -> RuntimeCommandError {
    RuntimeCommandError::InvalidCommand
}
