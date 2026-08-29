use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{
    content, digest, enumeration, fields, limits, non_empty, optional_content, unsigned, usage,
    EMPTY,
};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "turn.started" => started(value),
        "turn.input" => input(value),
        "turn.cancel_requested" => cancel(value),
        "turn.suspended" => suspended(value, true),
        "turn.completed" => completed(value, true),
        "turn.stopped" => stopped(value, true),
        "turn.failed" => failed(value, true),
        "execution.started" => execution_started(value),
        "execution.abandoned" => abandoned(value),
        "execution.completed" => completed(value, false),
        "execution.suspended" => suspended(value, false),
        "execution.stopped" => stopped(value, false),
        "execution.failed" => failed(value, false),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn started(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "kind",
            "agent_instance_id",
            "definition_id",
            "definition_revision",
            "snapshot_digest",
            "trusted_input_digest",
        ],
        &["prior_suspension_id"],
    )?;
    for key in [
        "command_id",
        "agent_instance_id",
        "definition_id",
        "definition_revision",
    ] {
        non_empty(value, key)?;
    }
    digest(value, "snapshot_digest")?;
    digest(value, "trusted_input_digest")?;
    conditional_identity(
        value,
        "kind",
        "continue",
        "prior_suspension_id",
        &["start", "continue"],
    )
}

fn input(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(value, &["input_kind", "content"], &["suspension_id"])?;
    content(value, "content")?;
    conditional_identity(
        value,
        "input_kind",
        "continuation",
        "suspension_id",
        &["trusted_user", "trusted_system", "continuation"],
    )
}

fn conditional_identity(
    value: &Map<String, Value>,
    enum_key: &str,
    requiring: &str,
    identity: &str,
    allowed: &[&str],
) -> Result<(), LedgerError> {
    match (
        enumeration(value, enum_key, allowed)? == requiring,
        value.get(identity),
    ) {
        (false, None) => Ok(()),
        (true, Some(_)) => non_empty(value, identity),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn cancel(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["command_id", "reason", "requested_through_position"],
        EMPTY,
    )?;
    non_empty(value, "command_id")?;
    enumeration(
        value,
        "reason",
        &["user", "deadline", "shutdown", "operator", "policy"],
    )?;
    unsigned(value, "requested_through_position", false)
}

fn suspended(value: &Map<String, Value>, turn: bool) -> Result<(), LedgerError> {
    let required = if turn {
        &[
            "suspension_id",
            "execution_id",
            "reason",
            "continuation",
            "cumulative_usage",
        ][..]
    } else {
        &["suspension_id", "reason", "continuation", "usage"][..]
    };
    fields(value, required, EMPTY)?;
    non_empty(value, "suspension_id")?;
    if turn {
        non_empty(value, "execution_id")?;
    }
    enumeration(
        value,
        "reason",
        &[
            "approval_required",
            "external_input_required",
            "operator_reconciliation",
            "resource_unavailable",
            "partial_output",
        ],
    )?;
    content(value, "continuation")?;
    usage(value, if turn { "cumulative_usage" } else { "usage" })
}

fn completed(value: &Map<String, Value>, turn: bool) -> Result<(), LedgerError> {
    let required = if turn {
        &["execution_id", "response", "cumulative_usage"][..]
    } else {
        &["response", "usage"][..]
    };
    fields(value, required, EMPTY)?;
    if turn {
        non_empty(value, "execution_id")?;
    }
    content(value, "response")?;
    usage(value, if turn { "cumulative_usage" } else { "usage" })
}

fn stopped(value: &Map<String, Value>, turn: bool) -> Result<(), LedgerError> {
    let required = if turn {
        &["execution_id", "reason", "cumulative_usage"][..]
    } else {
        &["reason", "usage"][..]
    };
    fields(value, required, &["evidence"])?;
    if turn {
        non_empty(value, "execution_id")?;
    }
    enumeration(
        value,
        "reason",
        &[
            "iteration_limit",
            "token_limit",
            "deadline",
            "cancelled",
            "resource_unavailable",
        ],
    )?;
    optional_content(value, "evidence")?;
    usage(value, if turn { "cumulative_usage" } else { "usage" })
}

fn failed(value: &Map<String, Value>, turn: bool) -> Result<(), LedgerError> {
    let required = if turn {
        &["execution_id", "reason", "cumulative_usage"][..]
    } else {
        &["reason", "usage"][..]
    };
    fields(value, required, &["evidence"])?;
    if turn {
        non_empty(value, "execution_id")?;
    }
    enumeration(
        value,
        "reason",
        &[
            "invalid_input",
            "invalid_model_output",
            "required_capability_unavailable",
            "port_failure",
            "invariant_violation",
            "durability_failure",
            "corrupt_recovery_state",
        ],
    )?;
    optional_content(value, "evidence")?;
    usage(value, if turn { "cumulative_usage" } else { "usage" })
}

fn execution_started(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "snapshot_digest",
            "through_position",
            "completed_iterations",
            "limits",
            "recovery_ordinal",
        ],
        EMPTY,
    )?;
    digest(value, "snapshot_digest")?;
    for key in [
        "through_position",
        "completed_iterations",
        "recovery_ordinal",
    ] {
        unsigned(value, key, false)?;
    }
    limits(value, "limits")
}

fn abandoned(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &["reason", "last_safe_position", "recovery_ordinal"],
        EMPTY,
    )?;
    enumeration(value, "reason", &["runtime_lost"])?;
    unsigned(value, "last_safe_position", false)?;
    unsigned(value, "recovery_ordinal", true)
}
