use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{
    content, digest, enumeration, fields, non_empty, object, optional_non_empty, unsigned, EMPTY,
};

const KINDS: &[&str] = &[
    "preference",
    "constraint",
    "decision",
    "learned_fact",
    "summary",
];
const SENSITIVITIES: &[&str] = &["ordinary", "restricted"];

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "memory.proposed" => proposal(value),
        "memory.committed" => committed(value),
        "memory.rejected" => rejected(value),
        "memory.superseded" => superseded(value),
        "memory.tombstoned" => tombstoned(value),
        "memory.retrieval_recorded" => retrieval(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn proposal(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "proposal_id",
            "namespace_id",
            "scope",
            "kind",
            "content",
            "evidence",
            "sensitivity",
            "confidence_basis_points",
        ],
        &["expected_active_revision_id"],
    )?;
    non_empty(value, "proposal_id")?;
    non_empty(value, "namespace_id")?;
    scope(value, "scope")?;
    enumeration(value, "kind", KINDS)?;
    content(value, "content")?;
    evidence(value, "evidence")?;
    enumeration(value, "sensitivity", SENSITIVITIES)?;
    basis_points(value, "confidence_basis_points")?;
    optional_non_empty(value, "expected_active_revision_id")
}

fn committed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "proposal_id",
            "record_id",
            "revision_id",
            "namespace_id",
            "scope",
            "kind",
            "content",
            "evidence",
            "sensitivity",
            "confidence_basis_points",
            "valid_from_position",
            "retention_policy_digest",
        ],
        &["expires_at_utc", "supersedes_revision_id"],
    )?;
    for key in ["proposal_id", "record_id", "revision_id", "namespace_id"] {
        non_empty(value, key)?;
    }
    scope(value, "scope")?;
    enumeration(value, "kind", KINDS)?;
    content(value, "content")?;
    evidence(value, "evidence")?;
    enumeration(value, "sensitivity", SENSITIVITIES)?;
    basis_points(value, "confidence_basis_points")?;
    unsigned(value, "valid_from_position", true)?;
    digest(value, "retention_policy_digest")?;
    optional_non_empty(value, "supersedes_revision_id")?;
    optional_timestamp(value, "expires_at_utc")
}

fn rejected(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["proposal_id", "reason"], EMPTY)?;
    non_empty(value, "proposal_id")?;
    enumeration(
        value,
        "reason",
        &[
            "namespace_denied",
            "evidence_not_found",
            "evidence_mismatch",
            "revision_conflict",
            "retention_rejected",
            "sensitivity_denied",
            "limit_exceeded",
            "unsupported",
        ],
    )?;
    Ok(())
}

fn superseded(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "record_id",
            "old_revision_id",
            "new_revision_id",
            "proposal_id",
        ],
        EMPTY,
    )?;
    for key in [
        "record_id",
        "old_revision_id",
        "new_revision_id",
        "proposal_id",
    ] {
        non_empty(value, key)?;
    }
    if value["old_revision_id"] == value["new_revision_id"] {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}

fn tombstoned(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["command_id", "record_id", "revision_id", "reason"],
        EMPTY,
    )?;
    for key in ["command_id", "record_id", "revision_id"] {
        non_empty(value, key)?;
    }
    enumeration(
        value,
        "reason",
        &[
            "expired",
            "superseded",
            "user_request",
            "policy",
            "corrupt_source",
        ],
    )?;
    Ok(())
}

fn retrieval(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "query_id",
            "query_digest",
            "namespace_id",
            "retriever_revision",
            "through_position",
            "as_of_utc",
            "max_results",
            "max_total_bytes",
            "include_restricted",
            "matches",
            "truncated",
        ],
        &["restricted_grant_digest"],
    )?;
    for key in ["query_id", "namespace_id", "retriever_revision"] {
        non_empty(value, key)?;
    }
    digest(value, "query_digest")?;
    unsigned(value, "through_position", false)?;
    timestamp(value, "as_of_utc")?;
    unsigned(value, "max_results", true)?;
    unsigned(value, "max_total_bytes", true)?;
    let include = boolean(value, "include_restricted")?;
    if include != value.contains_key("restricted_grant_digest") {
        return Err(LedgerError::InvalidFact);
    }
    if include {
        digest(value, "restricted_grant_digest")?;
    }
    let matches = value
        .get("matches")
        .and_then(Value::as_array)
        .ok_or(LedgerError::InvalidFact)?;
    for item in matches {
        let item = object(item)?;
        fields(
            item,
            &[
                "record_id",
                "revision_id",
                "content",
                "content_byte_length",
                "evidence",
                "relevance_basis_points",
                "sensitivity",
            ],
            EMPTY,
        )?;
        non_empty(item, "record_id")?;
        non_empty(item, "revision_id")?;
        content(item, "content")?;
        unsigned(item, "content_byte_length", true)?;
        if let Some(text) = object(item.get("content").unwrap())?
            .get("inline_utf8")
            .and_then(Value::as_str)
        {
            if item["content_byte_length"].as_u64() != Some(text.len() as u64) {
                return Err(LedgerError::InvalidFact);
            }
        }
        evidence(item, "evidence")?;
        basis_points(item, "relevance_basis_points")?;
        let sensitivity = enumeration(item, "sensitivity", SENSITIVITIES)?;
        if sensitivity == "restricted" && !include {
            return Err(LedgerError::InvalidFact);
        }
    }
    boolean(value, "truncated")?;
    Ok(())
}

fn scope(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let scope = object(value.get(key).ok_or(LedgerError::InvalidFact)?)?;
    match scope.get("kind").and_then(Value::as_str) {
        Some("namespace") => fields(scope, &["kind"], EMPTY),
        Some("session" | "agent_instance") => {
            fields(scope, &["kind", "owner_id"], EMPTY)?;
            non_empty(scope, "owner_id")
        }
        _ => Err(LedgerError::InvalidFact),
    }
}

fn evidence(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    let evidence = value
        .get(key)
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or(LedgerError::InvalidFact)?;
    for item in evidence {
        let item = object(item)?;
        fields(
            item,
            &["session_id", "position", "fact_id", "payload_digest"],
            EMPTY,
        )?;
        non_empty(item, "session_id")?;
        unsigned(item, "position", true)?;
        non_empty(item, "fact_id")?;
        digest(item, "payload_digest")?;
    }
    Ok(())
}

fn basis_points(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    unsigned(value, key, false)?;
    if value[key].as_u64().is_some_and(|points| points <= 10_000) {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}

fn boolean(value: &Map<String, Value>, key: &str) -> Result<bool, LedgerError> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or(LedgerError::InvalidFact)
}

fn optional_timestamp(value: &Map<String, Value>, key: &str) -> Result<(), LedgerError> {
    if value.contains_key(key) {
        timestamp(value, key)
    } else {
        Ok(())
    }
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
