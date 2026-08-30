use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{
    content, digest, enumeration, fields, non_empty, optional_content, optional_non_empty, string,
    EMPTY,
};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "interaction.requested" => interaction_requested(value),
        "interaction.resolved" => interaction_resolved(value),
        "interaction.cancelled" => interaction_cancelled(value),
        "tool.preparation_rejected" => preparation_rejected(value),
        "effect.prepared" => prepared(value),
        "effect.authorized" => authorized(value),
        "effect.denied" => denied(value),
        "effect.started" => started(value),
        "effect.receipt" => receipt(value),
        "effect.completed" => completed(value),
        "effect.failed" => failed(value),
        "effect.uncertain" => uncertain(value),
        "effect.reconciled" => reconciled(value),
        "effect.observation" => observation(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

pub(super) fn validate_prepared_v2(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_contract_version",
            "prepared_digest",
            "tool_name",
            "tool_revision",
            "replay_class",
            "model_call_id",
            "access_policy_revision",
            "access_resolver_revision",
            "invocation_accesses",
            "max_result_bytes",
        ],
        EMPTY,
    )?;
    if value
        .get("prepared_contract_version")
        .and_then(Value::as_u64)
        != Some(2)
    {
        return Err(LedgerError::InvalidFact);
    }
    digest(value, "prepared_digest")?;
    identities(
        value,
        &[
            "tool_name",
            "tool_revision",
            "model_call_id",
            "access_policy_revision",
            "access_resolver_revision",
        ],
    )?;
    enumeration(
        value,
        "replay_class",
        &[
            "read_only",
            "idempotent",
            "receipt_recoverable",
            "never_replay",
        ],
    )?;
    content(value, "invocation_accesses")?;
    super::values::unsigned(value, "max_result_bytes", true)
}

pub(super) fn validate_prepared_v3(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_contract_version",
            "prepared_digest",
            "tool_name",
            "tool_revision",
            "replay_class",
            "model_call_id",
            "arguments",
            "access_policy_revision",
            "access_resolver_revision",
            "invocation_accesses",
            "max_result_bytes",
            "sandbox_requirements",
            "sandbox_requirements_digest",
        ],
        EMPTY,
    )?;
    if value
        .get("prepared_contract_version")
        .and_then(Value::as_u64)
        != Some(3)
    {
        return Err(LedgerError::InvalidFact);
    }
    digest(value, "prepared_digest")?;
    content(value, "arguments")?;
    identities(
        value,
        &[
            "tool_name",
            "tool_revision",
            "model_call_id",
            "access_policy_revision",
            "access_resolver_revision",
        ],
    )?;
    enumeration(
        value,
        "replay_class",
        &[
            "read_only",
            "idempotent",
            "receipt_recoverable",
            "never_replay",
        ],
    )?;
    content(value, "invocation_accesses")?;
    super::values::unsigned(value, "max_result_bytes", true)?;
    content(value, "sandbox_requirements")?;
    digest(value, "sandbox_requirements_digest")?;
    if value
        .get("sandbox_requirements")
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("digest"))
        != value.get("sandbox_requirements_digest")
    {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}

pub(super) fn validate_authorized_v2(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_contract_version",
            "prepared_digest",
            "grant_id",
            "authority_revision",
            "constraints_digest",
            "granted_requirements",
        ],
        EMPTY,
    )?;
    if value
        .get("prepared_contract_version")
        .and_then(Value::as_u64)
        != Some(3)
    {
        return Err(LedgerError::InvalidFact);
    }
    digest(value, "prepared_digest")?;
    identities(value, &["grant_id", "authority_revision"])?;
    digest(value, "constraints_digest")?;
    content(value, "granted_requirements")
}

fn interaction_requested(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "interaction_id",
            "suspension_id",
            "prepared_digest",
            "kind",
            "prompt",
            "response_schema",
            "response_schema_digest",
            "expiry_code",
        ],
        &["response_schema"],
    )?;
    identities(value, &["interaction_id", "suspension_id"])?;
    digests(value, &["prepared_digest", "response_schema_digest"])?;
    optional_content(value, "response_schema")?;
    enumeration(value, "kind", &["approval", "external_input"])?;
    enumeration(
        value,
        "expiry_code",
        &["none", "turn_deadline", "policy_deadline"],
    )?;
    content(value, "prompt")?;
    content(value, "response_schema")
}

fn interaction_resolved(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "interaction_id",
            "suspension_id",
            "prepared_digest",
            "response",
        ],
        EMPTY,
    )?;
    identities(value, &["interaction_id", "suspension_id"])?;
    digest(value, "prepared_digest")?;
    content(value, "response")
}

fn interaction_cancelled(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "interaction_id",
            "suspension_id",
            "prepared_digest",
            "reason",
        ],
        EMPTY,
    )?;
    identities(value, &["interaction_id", "suspension_id"])?;
    digest(value, "prepared_digest")?;
    enumeration(
        value,
        "reason",
        &["user", "expired", "turn_cancelled", "operator"],
    )?;
    Ok(())
}

fn preparation_rejected(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "source_model_request_id",
            "model_call_id",
            "proposed_tool_name",
            "code",
            "failure_paths",
        ],
        EMPTY,
    )?;
    identities(value, &["source_model_request_id", "model_call_id"])?;
    string(value, "proposed_tool_name")?;
    enumeration(
        value,
        "code",
        &[
            "invalid_tool_name",
            "tool_not_admitted",
            "invalid_arguments_json",
            "arguments_schema_mismatch",
            "non_canonical_value",
        ],
    )?;
    content(value, "failure_paths")
}

fn prepared(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_digest",
            "tool_name",
            "tool_revision",
            "replay_class",
            "model_call_id",
        ],
        EMPTY,
    )?;
    digest(value, "prepared_digest")?;
    identities(value, &["tool_name", "tool_revision", "model_call_id"])?;
    enumeration(
        value,
        "replay_class",
        &[
            "read_only",
            "idempotent",
            "receipt_recoverable",
            "never_replay",
        ],
    )?;
    Ok(())
}

fn authorized(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_digest",
            "grant_id",
            "authority_revision",
            "granted_requirements",
        ],
        EMPTY,
    )?;
    digest(value, "prepared_digest")?;
    identities(value, &["grant_id", "authority_revision"])?;
    content(value, "granted_requirements")
}

fn denied(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["prepared_digest", "code"], &["safe_details"])?;
    digest(value, "prepared_digest")?;
    enumeration(
        value,
        "code",
        &["authorization_denied", "replacement_required"],
    )?;
    optional_content(value, "safe_details")
}

fn started(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_digest",
            "grant_id",
            "executor_id",
            "executor_revision",
            "dispatch_attempt_id",
        ],
        EMPTY,
    )?;
    digest(value, "prepared_digest")?;
    identities(
        value,
        &[
            "grant_id",
            "executor_id",
            "executor_revision",
            "dispatch_attempt_id",
        ],
    )
}

fn receipt(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "receipt_id",
            "prepared_digest",
            "grant_id",
            "executor_id",
            "executor_revision",
            "classification",
            "result_or_evidence",
        ],
        EMPTY,
    )?;
    digest(value, "prepared_digest")?;
    identities(
        value,
        &["receipt_id", "grant_id", "executor_id", "executor_revision"],
    )?;
    enumeration(value, "classification", &["completed", "failed"])?;
    content(value, "result_or_evidence")
}

fn completed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["prepared_digest", "receipt_id", "result"], EMPTY)?;
    digest(value, "prepared_digest")?;
    non_empty(value, "receipt_id")?;
    content(value, "result")
}

fn failed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["prepared_digest", "code"],
        &["receipt_id", "evidence"],
    )?;
    digest(value, "prepared_digest")?;
    optional_non_empty(value, "receipt_id")?;
    enumeration(
        value,
        "code",
        &[
            "timeout",
            "cancelled",
            "tool_failure",
            "requirement_unsupported",
            "executor_unavailable",
        ],
    )?;
    optional_content(value, "evidence")
}

fn uncertain(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["prepared_digest", "reason"], &["evidence"])?;
    digest(value, "prepared_digest")?;
    enumeration(
        value,
        "reason",
        &[
            "started_without_receipt",
            "receipt_invalid",
            "executor_state_unknown",
        ],
    )?;
    optional_content(value, "evidence")
}

fn reconciled(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "prepared_digest",
            "decision",
            "operator_evidence",
            "observation",
        ],
        EMPTY,
    )?;
    digest(value, "prepared_digest")?;
    enumeration(value, "decision", &["completed", "failed"])?;
    content(value, "operator_evidence")?;
    content(value, "observation")
}

fn observation(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["prepared_digest", "model_call_id", "observation"],
        EMPTY,
    )?;
    digest(value, "prepared_digest")?;
    non_empty(value, "model_call_id")?;
    content(value, "observation")
}

fn identities(value: &Map<String, Value>, keys: &[&str]) -> Result<(), LedgerError> {
    for key in keys {
        non_empty(value, key)?;
    }
    Ok(())
}

fn digests(value: &Map<String, Value>, keys: &[&str]) -> Result<(), LedgerError> {
    for key in keys {
        digest(value, key)?;
    }
    Ok(())
}
