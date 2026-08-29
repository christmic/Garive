use chrono::{DateTime, SecondsFormat, Utc};
use garive_ledger::{CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, TurnId};
use garive_memory::{retrieve_memory, MemoryQuery, MemoryRecord, MemoryRetrieval, MemoryScore};
use serde_json::{json, Value};

use crate::RuntimeCommandError;

use super::encoding::digest;

/// Durable ownership for one bounded M0 retrieval.
pub struct MemoryRetrievalContext {
    /// Turn that will consume the attributed results.
    pub turn_id: TurnId,
    /// Execution that will consume the attributed results.
    pub execution_id: ExecutionId,
    /// Canonical RFC 3339 UTC observation time supplied by Runtime.
    pub recorded_at: String,
}

/// Pure bounded retrieval paired with its commit-before-context fact.
pub struct PlannedMemoryRetrieval {
    /// Exact ordered matches supplied to Core only after commit.
    pub retrieval: MemoryRetrieval,
    /// Fact that must commit before any returned content enters a model request.
    pub fact: FactDraft,
}

/// Filters scored revisions and plans their exact durable retrieval binding.
pub fn plan_memory_retrieval(
    context: &MemoryRetrievalContext,
    records: &[MemoryRecord],
    scores: &[MemoryScore],
    query: &MemoryQuery,
) -> Result<PlannedMemoryRetrieval, RuntimeCommandError> {
    validate_time(&context.recorded_at)?;
    let retrieval =
        retrieve_memory(records, scores, query).map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let query_digest = query
        .query_digest()
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let matches = retrieval
        .matches
        .iter()
        .map(|value| {
            Ok(json!({
                "record_id": value.record_id(),
                "revision_id": value.revision_id(),
                "content": serde_json::to_value(value.content()).map_err(|_| RuntimeCommandError::InvariantViolation)?,
                "content_byte_length": value.content_byte_length(),
                "evidence": serde_json::to_value(value.evidence()).map_err(|_| RuntimeCommandError::InvariantViolation)?,
                "relevance_basis_points": value.relevance_basis_points(),
                "sensitivity": serde_json::to_value(value.sensitivity()).map_err(|_| RuntimeCommandError::InvariantViolation)?,
            }))
        })
        .collect::<Result<Vec<Value>, RuntimeCommandError>>()?;
    let mut payload = json!({
        "query_id": query.query_id(),
        "query_digest": query_digest,
        "namespace_id": query.namespace_id(),
        "retriever_revision": query.retriever_revision(),
        "through_position": query.through_position(),
        "as_of_utc": query.as_of_utc(),
        "max_results": query.max_results(),
        "max_total_bytes": query.max_total_bytes(),
        "include_restricted": query.include_restricted(),
        "matches": matches,
        "truncated": retrieval.truncated,
    });
    if let Some(grant) = query.restricted_grant_digest() {
        payload
            .as_object_mut()
            .ok_or(RuntimeCommandError::InvariantViolation)?
            .insert("restricted_grant_digest".into(), json!(grant));
    }
    let fact_digest = digest(format!("memory.retrieval_recorded:{}", query.query_id()).as_bytes());
    let fact = FactDraft {
        fact_id: FactId::try_from(format!("fact-{fact_digest}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: Some(context.turn_id.clone()),
        execution_id: Some(context.execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("memory.retrieval_recorded")
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: context.recorded_at.clone(),
    };
    Ok(PlannedMemoryRetrieval { retrieval, fact })
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
