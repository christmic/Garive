use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};

use super::values::{
    content, digest, enumeration, fields, non_empty, object, optional_non_empty, unsigned, EMPTY,
};
use crate::LedgerError;

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "knowledge.requested" => requested(value),
        "knowledge.dispatched" => dispatched(value),
        "knowledge.completed" => completed(value),
        "knowledge.failed" => failed(value),
        _ => Err(LedgerError::InvalidFact),
    }
}
fn dispatched(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["request_id", "request_digest", "dispatch_attempt_id"],
        EMPTY,
    )?;
    non_empty(value, "request_id")?;
    digest(value, "request_digest")?;
    non_empty(value, "dispatch_attempt_id")
}
fn requested(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "source_id",
            "source_revision",
            "request_digest",
            "mode",
            "query",
            "filters",
            "through_position",
            "max_chunks",
            "max_total_bytes",
            "deadline_budget_ms",
            "freshness_kind",
        ],
        &["exact_snapshot_digest"],
    )?;
    for key in ["request_id", "source_id", "source_revision"] {
        non_empty(value, key)?;
    }
    digest(value, "request_digest")?;
    enumeration(value, "mode", &["keyword", "semantic", "structured"])?;
    content(value, "query")?;
    content(value, "filters")?;
    unsigned(value, "through_position", false)?;
    for key in ["max_chunks", "max_total_bytes", "deadline_budget_ms"] {
        unsigned(value, key, true)?;
    }
    let freshness = enumeration(
        value,
        "freshness_kind",
        &["cached_allowed", "revalidate", "exact_snapshot"],
    )?;
    if (freshness == "exact_snapshot") != value.contains_key("exact_snapshot_digest") {
        return Err(LedgerError::InvalidFact);
    }
    if freshness == "exact_snapshot" {
        digest(value, "exact_snapshot_digest")?;
    }
    Ok(())
}
fn completed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["request_id", "request_digest", "evidence", "truncated"],
        EMPTY,
    )?;
    non_empty(value, "request_id")?;
    digest(value, "request_digest")?;
    for item in value
        .get("evidence")
        .and_then(Value::as_array)
        .ok_or(LedgerError::InvalidFact)?
    {
        evidence(object(item)?)?;
    }
    value
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or(LedgerError::InvalidFact)?;
    Ok(())
}
fn evidence(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "evidence_id",
            "content",
            "content_byte_length",
            "citation_kind",
            "citation_locator",
            "citation_content_digest",
            "retrieved_at_utc",
            "freshness",
            "trust_class",
            "rank_basis_points",
        ],
        &["source_snapshot_digest", "citation_title", "canonical_uri"],
    )?;
    non_empty(value, "evidence_id")?;
    if value.contains_key("source_snapshot_digest") {
        digest(value, "source_snapshot_digest")?;
    }
    content(value, "content")?;
    unsigned(value, "content_byte_length", true)?;
    let binding = object(value.get("content").unwrap())?;
    if let Some(text) = binding.get("inline_utf8").and_then(Value::as_str) {
        if value["content_byte_length"].as_u64() != Some(text.len() as u64) {
            return Err(LedgerError::InvalidFact);
        }
    }
    enumeration(
        value,
        "citation_kind",
        &[
            "uri_fragment",
            "document_offset",
            "record_key",
            "opaque_locator",
        ],
    )?;
    non_empty(value, "citation_locator")?;
    optional_non_empty(value, "citation_title")?;
    optional_non_empty(value, "canonical_uri")?;
    digest(value, "citation_content_digest")?;
    if value["citation_content_digest"] != binding["digest"] {
        return Err(LedgerError::InvalidFact);
    }
    timestamp(value, "retrieved_at_utc")?;
    enumeration(value, "freshness", &["fresh", "cached", "stale"])?;
    enumeration(
        value,
        "trust_class",
        &["curated", "first_party", "third_party", "untrusted"],
    )?;
    unsigned(value, "rank_basis_points", false)?;
    if !value["rank_basis_points"]
        .as_u64()
        .is_some_and(|rank| rank <= 10_000)
    {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}
fn failed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "request_digest",
            "phase",
            "reason",
            "ambiguous",
        ],
        &["retry_after_ms"],
    )?;
    non_empty(value, "request_id")?;
    digest(value, "request_digest")?;
    let phase = enumeration(
        value,
        "phase",
        &["pre_dispatch", "dispatched", "response_validation"],
    )?;
    let reason = enumeration(
        value,
        "reason",
        &[
            "invalid_query",
            "source_not_found",
            "source_revision_mismatch",
            "source_denied",
            "filter_unsupported",
            "freshness_unavailable",
            "connector_unavailable",
            "connector_rejected",
            "retrieval_uncertain",
            "citation_invalid",
            "content_digest_mismatch",
            "limit_exceeded",
            "durability_failure",
            "corrupt_knowledge_state",
        ],
    )?;
    let ambiguous = value
        .get("ambiguous")
        .and_then(Value::as_bool)
        .ok_or(LedgerError::InvalidFact)?;
    if ambiguous != (phase == "dispatched" && reason == "retrieval_uncertain") {
        return Err(LedgerError::InvalidFact);
    }
    if value.contains_key("retry_after_ms") {
        unsigned(value, "retry_after_ms", true)?;
    }
    Ok(())
}
fn timestamp(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let raw = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(LedgerError::InvalidFact)?;
    if DateTime::parse_from_rfc3339(raw).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == raw
    }) {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}
