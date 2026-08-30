use serde_json::{Map, Value};

use crate::LedgerError;

use super::{
    plan_step,
    values::{content, digest, fields, non_empty, optional_content, optional_unsigned, unsigned},
};

const BASE: &[&str] = &["command_id", "plan_id", "plan_revision"];

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "plan.proposed" => proposed(value),
        "plan.adopted" => adopted(value),
        "plan.rejected" => rejected(value),
        "plan.superseded" => superseded(value),
        "plan.suspended" => suspended(value),
        "plan.resumed" => resumed(value),
        "plan.completed" => completed(value),
        "plan.failed" => failed(value),
        step_kind if step_kind.starts_with("plan.step.") => plan_step::validate(step_kind, value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn proposed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "plan_id",
            "plan_revision",
            "state_version",
            "plan_digest",
            "definition",
            "goal_id",
            "goal_revision",
            "goal_definition_digest",
            "agent_snapshot_digest",
            "tool_catalogue_digest",
            "safety_policy_revision",
            "proposer_reference",
        ],
        &[],
    )?;
    base(value)?;
    if value.get("state_version").and_then(Value::as_u64) != Some(1) {
        return Err(LedgerError::InvalidFact);
    }
    for key in [
        "plan_digest",
        "goal_definition_digest",
        "agent_snapshot_digest",
        "tool_catalogue_digest",
    ] {
        digest(value, key)?;
    }
    content(value, "definition")?;
    non_empty(value, "goal_id")?;
    unsigned(value, "goal_revision", true)?;
    non_empty(value, "safety_policy_revision")?;
    non_empty(value, "proposer_reference")
}

fn adopted(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "command_id",
            "plan_id",
            "plan_revision",
            "previous_state_version",
            "state_version",
            "expected_goal_revision",
            "actor_reference",
            "policy_reference",
            "carry_forward_evidence",
        ],
        &["expected_prior_plan_revision"],
    )?;
    mutation(value)?;
    unsigned(value, "expected_goal_revision", true)?;
    optional_unsigned(value, "expected_prior_plan_revision")?;
    if value.get("expected_prior_plan_revision") == Some(&Value::from(0)) {
        return Err(LedgerError::InvalidFact);
    }
    non_empty(value, "actor_reference")?;
    non_empty(value, "policy_reference")?;
    content(value, "carry_forward_evidence")
}

fn rejected(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(value, &["reason"], &[])?;
    non_empty(value, "reason")
}

fn superseded(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "replacement_plan_id",
            "replacement_plan_revision",
            "replacement_plan_digest",
            "unresolved_work",
        ],
        &[],
    )?;
    non_empty(value, "replacement_plan_id")?;
    unsigned(value, "replacement_plan_revision", true)?;
    digest(value, "replacement_plan_digest")?;
    content(value, "unresolved_work")
}

fn suspended(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(value, &["continuation_kind", "continuation_reference"], &[])?;
    continuation(value)
}

fn resumed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(value, &["resolved_continuation_reference"], &[])?;
    non_empty(value, "resolved_continuation_reference")
}

fn completed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(value, &["reduction_evidence"], &[])?;
    content(value, "reduction_evidence")
}

fn failed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(value, &["reason"], &["evidence"])?;
    non_empty(value, "reason")?;
    optional_content(value, "evidence")
}

pub(super) fn base(value: &Map<String, Value>) -> Result<(), LedgerError> {
    non_empty(value, "command_id")?;
    non_empty(value, "plan_id")?;
    unsigned(value, "plan_revision", true)
}

pub(super) fn mutation(value: &Map<String, Value>) -> Result<(), LedgerError> {
    base(value)?;
    unsigned(value, "previous_state_version", true)?;
    unsigned(value, "state_version", true)?;
    if value
        .get("previous_state_version")
        .and_then(Value::as_u64)
        .and_then(|previous| previous.checked_add(1))
        != value.get("state_version").and_then(Value::as_u64)
    {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}

pub(super) fn mutation_with(
    value: &Map<String, Value>,
    additional: &[&str],
    optional: &[&str],
) -> Result<(), LedgerError> {
    let mut required = BASE.to_vec();
    required.extend(["previous_state_version", "state_version"]);
    required.extend(additional);
    fields(value, &required, optional)?;
    mutation(value)
}

pub(super) fn continuation(value: &Map<String, Value>) -> Result<(), LedgerError> {
    super::values::enumeration(
        value,
        "continuation_kind",
        &["interaction", "reconciliation"],
    )?;
    non_empty(value, "continuation_reference")
}
