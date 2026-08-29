use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{content, digest, enumeration, fields, non_empty, object, unsigned, EMPTY};

const FAILURES: &[&str] = &[
    "invalid_delegation",
    "child_not_found",
    "child_revision_mismatch",
    "authority_denied",
    "budget_exhausted",
    "budget_overflow",
    "depth_exceeded",
    "concurrency_exceeded",
    "result_schema_mismatch",
    "delegation_conflict",
    "child_state_corrupt",
    "durability_failure",
    "corrupt_delegation_state",
];

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "delegation.requested" => requested(value),
        "delegation.authorized" => authorized(value),
        "delegation.denied" => denied(value),
        "delegation.child_started" => child_started(value),
        "delegation.child_terminal" => child_terminal(value),
        "delegation.observed" => observed(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn requested(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "delegation_id",
            "parent_agent_instance_id",
            "intent",
            "intent_digest",
            "through_position",
        ],
        EMPTY,
    )?;
    non_empty(value, "delegation_id")?;
    non_empty(value, "parent_agent_instance_id")?;
    content(value, "intent")?;
    digest(value, "intent_digest")?;
    unsigned(value, "through_position", false)?;
    same_binding_digest(value, "intent", "intent_digest")
}

fn authorized(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "delegation_id",
            "grant_id",
            "intent_digest",
            "reserved_budget",
            "authority_revision",
        ],
        EMPTY,
    )?;
    for key in ["delegation_id", "grant_id", "authority_revision"] {
        non_empty(value, key)?;
    }
    digest(value, "intent_digest")?;
    budget(object(
        value
            .get("reserved_budget")
            .ok_or(LedgerError::InvalidFact)?,
    )?)
}

fn denied(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["delegation_id", "intent_digest", "code"], EMPTY)?;
    non_empty(value, "delegation_id")?;
    digest(value, "intent_digest")?;
    enumeration(value, "code", FAILURES)?;
    Ok(())
}

fn child_started(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "delegation_id",
            "grant_id",
            "suspension_id",
            "child_agent_instance_id",
            "child_turn_id",
            "child_snapshot_digest",
        ],
        EMPTY,
    )?;
    for key in [
        "delegation_id",
        "grant_id",
        "suspension_id",
        "child_agent_instance_id",
        "child_turn_id",
    ] {
        non_empty(value, key)?;
    }
    digest(value, "child_snapshot_digest")
}

fn child_terminal(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "delegation_id",
            "grant_id",
            "result_id",
            "suspension_id",
            "child_agent_instance_id",
            "child_turn_id",
            "result",
            "result_digest",
        ],
        EMPTY,
    )?;
    for key in [
        "delegation_id",
        "grant_id",
        "result_id",
        "suspension_id",
        "child_agent_instance_id",
        "child_turn_id",
    ] {
        non_empty(value, key)?;
    }
    content(value, "result")?;
    digest(value, "result_digest")?;
    same_binding_digest(value, "result", "result_digest")
}

fn observed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "delegation_id",
            "grant_id",
            "result_id",
            "suspension_id",
            "result_digest",
        ],
        EMPTY,
    )?;
    for key in ["delegation_id", "grant_id", "result_id", "suspension_id"] {
        non_empty(value, key)?;
    }
    digest(value, "result_digest")
}

fn budget(value: &Map<String, Value>) -> Result<(), LedgerError> {
    const FIELDS: &[&str] = &[
        "max_child_turns",
        "max_child_executions",
        "max_iterations",
        "max_input_tokens",
        "max_output_tokens",
        "deadline_budget_ms",
        "max_depth",
        "max_objective_bytes",
        "max_input_evidence",
        "max_result_schema_bytes",
        "max_result_bytes",
        "max_result_evidence",
    ];
    fields(value, FIELDS, EMPTY)?;
    for key in FIELDS {
        unsigned(value, key, true)?;
    }
    if value["max_child_turns"].as_u64() == Some(1) {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}

fn same_binding_digest(
    value: &Map<String, Value>,
    binding: &str,
    asserted: &str,
) -> Result<(), LedgerError> {
    if value[binding]["digest"] == value[asserted] {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}
