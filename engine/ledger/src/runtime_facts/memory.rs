use chrono::{DateTime, SecondsFormat, Utc};
use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{
    content, digest, enumeration, fields, non_empty, object, optional_non_empty, string, unsigned,
    EMPTY,
};

const KINDS: &[&str] = &[
    "preference",
    "constraint",
    "decision",
    "learned_fact",
    "summary",
];
const SENSITIVITIES: &[&str] = &["ordinary", "restricted"];
const MEMORY_TYPES: &[&str] = &["semantic", "episodic", "lesson", "procedural"];
const AUTHORITIES: &[&str] = &["user_declared", "agent_learned", "organisation_published"];
const STATES: &[&str] = &["candidate", "active", "cold", "archived", "promoted"];
const CONTROL_SCOPES: &[&str] = &["session", "agent_instance", "user", "project", "platform"];

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "memory.proposed" => proposal(value),
        "memory.committed" => committed(value),
        "memory.revision_classified" => revision_classified(value),
        "memory.rejected" => rejected(value),
        "memory.superseded" => superseded(value),
        "memory.tombstoned" => tombstoned(value),
        "memory.retrieval_recorded" => retrieval(value),
        "memory.recall_recorded" => recall(value),
        "memory.obligation_opened" => obligation(value),
        "memory.observation_recorded" => observation(value),
        "memory.lifecycle_transitioned" => lifecycle(value),
        "memory.candidate_recorded" => candidate(value),
        "memory.maintenance_decided" => maintenance_decision(value),
        "memory.distillation_checkpointed" => distillation_checkpoint(value),
        "memory.audit_recorded" => audit(value),
        "memory.promotion_requested" => promotion_request(value),
        "memory.promotion_recorded" => promotion_receipt(value),
        "memory.erasure_requested" => erasure_request(value),
        "memory.erasure_recorded" => erasure_receipt(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn revision_classified(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "classification_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "memory_type",
            "authority",
            "lifecycle",
            "scope",
            "scope_owner_id",
            "policy_revision",
            "source_commit",
        ],
        &["aggregation_policy_digest", "authority_receipt_digest"],
    )?;
    for key in [
        "classification_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "scope_owner_id",
        "policy_revision",
    ] {
        non_empty(value, key)?;
    }
    enumeration(value, "memory_type", MEMORY_TYPES)?;
    let authority = enumeration(value, "authority", AUTHORITIES)?;
    enumeration(value, "lifecycle", &["candidate", "active"])?;
    let scope = enumeration(value, "scope", CONTROL_SCOPES)?;
    fact_reference(object(
        value.get("source_commit").ok_or(LedgerError::InvalidFact)?,
    )?)?;
    let requires_receipt = authority != "agent_learned";
    if requires_receipt != value.contains_key("authority_receipt_digest") {
        return Err(LedgerError::InvalidFact);
    }
    if requires_receipt {
        digest(value, "authority_receipt_digest")?;
    }
    let platform = scope == "platform";
    if platform != value.contains_key("aggregation_policy_digest") {
        return Err(LedgerError::InvalidFact);
    }
    if platform {
        digest(value, "aggregation_policy_digest")?;
    }
    Ok(())
}

fn candidate(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "candidate_id",
            "namespace_id",
            "extractor_revision",
            "source",
            "intent_kind",
            "intent_digest",
        ],
        EMPTY,
    )?;
    for key in ["candidate_id", "namespace_id", "extractor_revision"] {
        non_empty(value, key)?;
    }
    enumeration(
        value,
        "source",
        &[
            "explicit_user_command",
            "session_end",
            "exit_summary",
            "scheduled_distillation",
        ],
    )?;
    enumeration(value, "intent_kind", &["learn", "forget"])?;
    digest(value, "intent_digest")
}

fn maintenance_decision(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "decision_id",
            "candidate_id",
            "namespace_id",
            "decision_kind",
            "decision_digest",
        ],
        EMPTY,
    )?;
    for key in ["decision_id", "candidate_id", "namespace_id"] {
        non_empty(value, key)?;
    }
    enumeration(value, "decision_kind", &["add", "update", "delete", "noop"])?;
    digest(value, "decision_digest")
}

fn distillation_checkpoint(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "checkpoint_id",
            "namespace_id",
            "extractor_revision",
            "session_id",
            "through_position",
            "batch_digest",
        ],
        EMPTY,
    )?;
    for key in [
        "checkpoint_id",
        "namespace_id",
        "extractor_revision",
        "session_id",
    ] {
        non_empty(value, key)?;
    }
    unsigned(value, "through_position", true)?;
    digest(value, "batch_digest")
}

fn audit(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "audit_id",
            "namespace_id",
            "through_position",
            "policy_digest",
            "inventory_digest",
            "report_digest",
            "action_count",
            "truncated",
        ],
        EMPTY,
    )?;
    for key in ["audit_id", "namespace_id"] {
        non_empty(value, key)?;
    }
    unsigned(value, "through_position", true)?;
    for key in ["policy_digest", "inventory_digest", "report_digest"] {
        digest(value, key)?;
    }
    unsigned(value, "action_count", false)?;
    boolean(value, "truncated")?;
    Ok(())
}

fn promotion_request(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "memory_type",
            "policy_revision",
            "knowledge_proposal_id",
            "evidence_digest",
        ],
        EMPTY,
    )?;
    for key in [
        "request_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "policy_revision",
        "knowledge_proposal_id",
    ] {
        non_empty(value, key)?;
    }
    enumeration(value, "memory_type", MEMORY_TYPES)?;
    digest(value, "evidence_digest")
}

fn promotion_receipt(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "knowledge_proposal_id",
            "knowledge_record_id",
            "knowledge_revision_id",
            "receipt_digest",
        ],
        EMPTY,
    )?;
    for key in [
        "request_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "knowledge_proposal_id",
        "knowledge_record_id",
        "knowledge_revision_id",
    ] {
        non_empty(value, key)?;
    }
    digest(value, "receipt_digest")
}

fn erasure_request(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "tombstone_fact",
            "policy_revision",
            "targets",
        ],
        EMPTY,
    )?;
    for key in [
        "request_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "policy_revision",
    ] {
        non_empty(value, key)?;
    }
    fact_reference(object(
        value
            .get("tombstone_fact")
            .ok_or(LedgerError::InvalidFact)?,
    )?)?;
    let targets = value
        .get("targets")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= 64)
        .ok_or(LedgerError::InvalidFact)?;
    let mut prior: Option<(usize, &str)> = None;
    for target in targets {
        let target = object(target)?;
        fields(target, &["target_id", "kind"], EMPTY)?;
        non_empty(target, "target_id")?;
        let kind = enumeration(
            target,
            "kind",
            &["primary_store", "projection", "cache", "backup"],
        )?;
        let order = match kind {
            "primary_store" => 0,
            "projection" => 1,
            "cache" => 2,
            _ => 3,
        };
        let id = string(target, "target_id")?;
        if prior.is_some_and(|(prior_order, prior_id)| {
            order < prior_order || order == prior_order && id <= prior_id
        }) {
            return Err(LedgerError::InvalidFact);
        }
        prior = Some((order, id));
    }
    Ok(())
}

fn erasure_receipt(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "attempt_id",
            "attempted_at_position",
            "results",
            "disposition",
        ],
        EMPTY,
    )?;
    for key in [
        "request_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "attempt_id",
    ] {
        non_empty(value, key)?;
    }
    unsigned(value, "attempted_at_position", true)?;
    let attempted = value["attempted_at_position"].as_u64().unwrap();
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= 64)
        .ok_or(LedgerError::InvalidFact)?;
    let mut targets = BTreeSet::new();
    let mut complete = true;
    for result in results {
        let result = object(result)?;
        fields(
            result,
            &["target_id", "status", "receipt_digest"],
            &["not_before_position"],
        )?;
        non_empty(result, "target_id")?;
        if !targets.insert(string(result, "target_id")?) {
            return Err(LedgerError::InvalidFact);
        }
        let status = enumeration(
            result,
            "status",
            &[
                "erased",
                "not_present",
                "pending_backup_retention",
                "pending_retry",
            ],
        )?;
        digest(result, "receipt_digest")?;
        let pending_backup = status == "pending_backup_retention";
        if pending_backup != result.contains_key("not_before_position") {
            return Err(LedgerError::InvalidFact);
        }
        if pending_backup {
            unsigned(result, "not_before_position", true)?;
            if result["not_before_position"].as_u64().unwrap() <= attempted {
                return Err(LedgerError::InvalidFact);
            }
        }
        complete &= matches!(status, "erased" | "not_present");
    }
    let disposition = enumeration(value, "disposition", &["complete", "partial"])?;
    if (disposition == "complete") != complete {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}

fn recall(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "selection_id",
            "request_digest",
            "namespace_id",
            "product",
            "selection_policy_revision",
            "through_position",
            "max_items",
            "max_total_bytes",
            "items",
            "truncated",
        ],
        &["exploration"],
    )?;
    for key in ["selection_id", "namespace_id", "selection_policy_revision"] {
        non_empty(value, key)?;
    }
    digest(value, "request_digest")?;
    let product = enumeration(value, "product", &["menu", "detail"])?;
    unsigned(value, "through_position", false)?;
    unsigned(value, "max_items", true)?;
    unsigned(value, "max_total_bytes", true)?;
    let exploration = value.get("exploration").map(object).transpose()?;
    if let Some(exploration) = exploration {
        fields(exploration, &["algorithm_revision", "seed", "slots"], EMPTY)?;
        if enumeration(exploration, "algorithm_revision", &["hash-explore-v1"])?
            != "hash-explore-v1"
        {
            return Err(LedgerError::InvalidFact);
        }
        unsigned(exploration, "seed", false)?;
        unsigned(exploration, "slots", true)?;
    }
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or(LedgerError::InvalidFact)?;
    if items.len() > value["max_items"].as_u64().unwrap() as usize {
        return Err(LedgerError::InvalidFact);
    }
    let mut bytes = 0_u64;
    for item in items {
        let item = object(item)?;
        fields(
            item,
            &[
                "record_id",
                "revision_id",
                "memory_type",
                "role",
                "authority",
                "state",
                "safe_label",
                "content_digest",
                "content_byte_length",
                "evidence_count",
                "relevance_basis_points",
                "recency_basis_points",
                "importance_basis_points",
                "selection_kind",
            ],
            &["draw_hex"],
        )?;
        for key in ["record_id", "revision_id", "safe_label"] {
            non_empty(item, key)?;
        }
        if item["safe_label"]
            .as_str()
            .is_none_or(|text| text.len() > 256)
        {
            return Err(LedgerError::InvalidFact);
        }
        enumeration(item, "memory_type", MEMORY_TYPES)?;
        enumeration(item, "role", KINDS)?;
        enumeration(item, "authority", AUTHORITIES)?;
        let state = enumeration(item, "state", STATES)?;
        if state == "promoted" || product == "menu" && state == "archived" {
            return Err(LedgerError::InvalidFact);
        }
        digest(item, "content_digest")?;
        unsigned(item, "content_byte_length", true)?;
        unsigned(item, "evidence_count", true)?;
        for key in [
            "relevance_basis_points",
            "recency_basis_points",
            "importance_basis_points",
        ] {
            basis_points(item, key)?;
        }
        let selection = enumeration(item, "selection_kind", &["ranked", "explored"])?;
        if (selection == "explored") != item.contains_key("draw_hex")
            || selection == "explored" && exploration.is_none()
        {
            return Err(LedgerError::InvalidFact);
        }
        if let Some(draw) = item.get("draw_hex").and_then(Value::as_str) {
            if draw.len() != 16
                || !draw
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(LedgerError::InvalidFact);
            }
        }
        bytes = bytes
            .checked_add(item["content_byte_length"].as_u64().unwrap())
            .ok_or(LedgerError::InvalidFact)?;
    }
    if bytes > value["max_total_bytes"].as_u64().unwrap() {
        return Err(LedgerError::InvalidFact);
    }
    boolean(value, "truncated")?;
    Ok(())
}

fn obligation(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "obligation_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "recall_fact",
            "selection_id",
            "application_fact",
            "expected_outcome_digest",
            "application_scope_digest",
            "attribution_policy_revision",
            "expires_at_position",
        ],
        EMPTY,
    )?;
    for key in [
        "obligation_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "selection_id",
        "attribution_policy_revision",
    ] {
        non_empty(value, key)?;
    }
    fact_reference(object(
        value.get("recall_fact").ok_or(LedgerError::InvalidFact)?,
    )?)?;
    fact_reference(object(
        value
            .get("application_fact")
            .ok_or(LedgerError::InvalidFact)?,
    )?)?;
    digest(value, "expected_outcome_digest")?;
    digest(value, "application_scope_digest")?;
    unsigned(value, "expires_at_position", true)?;
    Ok(())
}

fn observation(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "observation_id",
            "obligation_id",
            "namespace_id",
            "position",
            "verifier_revision",
            "evidence",
            "verdict",
        ],
        EMPTY,
    )?;
    for key in [
        "observation_id",
        "obligation_id",
        "namespace_id",
        "verifier_revision",
    ] {
        non_empty(value, key)?;
    }
    unsigned(value, "position", true)?;
    let evidence = value["evidence"]
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or(LedgerError::InvalidFact)?;
    for entry in evidence {
        let entry = object(entry)?;
        fields(entry, &["kind", "fact"], EMPTY)?;
        enumeration(
            entry,
            "kind",
            &[
                "tool_result",
                "test_result",
                "effect_receipt",
                "user_correction",
                "deterministic_verifier",
            ],
        )?;
        fact_reference(object(entry.get("fact").ok_or(LedgerError::InvalidFact)?)?)?;
    }
    let verdict = object(value.get("verdict").ok_or(LedgerError::InvalidFact)?)?;
    match verdict.get("kind").and_then(Value::as_str) {
        Some("verified") => fields(verdict, &["kind"], EMPTY),
        Some("neutral") => {
            fields(verdict, &["kind", "safe_reason"], EMPTY)?;
            non_empty(verdict, "safe_reason")
        }
        Some("falsified") => {
            fields(verdict, &["kind", "in_scope"], &["observed_scope_digest"])?;
            let in_scope = boolean(verdict, "in_scope")?;
            if in_scope == verdict.contains_key("observed_scope_digest") {
                return Err(LedgerError::InvalidFact);
            }
            if !in_scope {
                digest(verdict, "observed_scope_digest")?;
            }
            Ok(())
        }
        _ => Err(LedgerError::InvalidFact),
    }
}

fn lifecycle(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "transition_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "from_state",
            "to_state",
            "verified",
            "falsified",
            "neutral",
            "last_observed_position",
            "cause_kind",
            "cause_id",
        ],
        &["promoted_knowledge_receipt_digest"],
    )?;
    for key in [
        "transition_id",
        "namespace_id",
        "record_id",
        "revision_id",
        "cause_id",
    ] {
        non_empty(value, key)?;
    }
    enumeration(value, "from_state", STATES)?;
    let state = enumeration(value, "to_state", STATES)?;
    for key in ["verified", "falsified", "neutral"] {
        unsigned(value, key, false)?;
    }
    unsigned(value, "last_observed_position", true)?;
    enumeration(
        value,
        "cause_kind",
        &[
            "observation",
            "maintenance",
            "promotion",
            "toolchain_changed",
        ],
    )?;
    if (state == "promoted") != value.contains_key("promoted_knowledge_receipt_digest") {
        return Err(LedgerError::InvalidFact);
    }
    if state == "promoted" {
        digest(value, "promoted_knowledge_receipt_digest")?;
    }
    Ok(())
}

fn fact_reference(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["session_id", "position", "fact_id", "payload_digest"],
        EMPTY,
    )?;
    non_empty(value, "session_id")?;
    unsigned(value, "position", true)?;
    non_empty(value, "fact_id")?;
    digest(value, "payload_digest")
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
        &[
            "command_id",
            "namespace_id",
            "record_id",
            "revision_id",
            "reason",
        ],
        EMPTY,
    )?;
    for key in ["command_id", "namespace_id", "record_id", "revision_id"] {
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
