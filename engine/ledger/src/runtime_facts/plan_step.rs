use serde_json::{Map, Value};

use crate::LedgerError;

use super::{
    plan::{continuation, mutation_with},
    values::{content, digest, enumeration, non_empty, optional_content, unsigned},
};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "plan.step.claimed" => claimed(value),
        "plan.step.claim_expired" => claim_expired(value),
        "plan.step.started" => started(value),
        "plan.step.completed" => completed(value),
        "plan.step.failed" => failed(value),
        "plan.step.suspended" => suspended(value),
        "plan.step.resumed" => resumed(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn claimed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "step_id",
            "step_digest",
            "claim_id",
            "worker_reference",
            "lease_epoch",
            "clock_revision",
            "claimed_at_tick",
            "expires_at_tick",
        ],
        &[],
    )?;
    non_empty(value, "step_id")?;
    digest(value, "step_digest")?;
    non_empty(value, "claim_id")?;
    non_empty(value, "worker_reference")?;
    unsigned(value, "lease_epoch", true)?;
    non_empty(value, "clock_revision")?;
    unsigned(value, "claimed_at_tick", false)?;
    unsigned(value, "expires_at_tick", false)?;
    if value.get("expires_at_tick").and_then(Value::as_u64)
        <= value.get("claimed_at_tick").and_then(Value::as_u64)
    {
        return Err(LedgerError::InvalidFact);
    }
    Ok(())
}

fn claim_expired(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "step_id",
            "claim_id",
            "lease_epoch",
            "clock_revision",
            "observed_at_tick",
        ],
        &[],
    )?;
    non_empty(value, "step_id")?;
    non_empty(value, "claim_id")?;
    unsigned(value, "lease_epoch", true)?;
    non_empty(value, "clock_revision")?;
    unsigned(value, "observed_at_tick", false)
}

fn started(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "step_id",
            "step_digest",
            "claim_id",
            "lease_epoch",
            "clock_revision",
            "observed_at_tick",
            "attempt_id",
            "execution_id",
            "execution_snapshot_digest",
            "sandbox_profile_digest",
            "safety_decision_id",
        ],
        &[],
    )?;
    non_empty(value, "step_id")?;
    digest(value, "step_digest")?;
    non_empty(value, "claim_id")?;
    unsigned(value, "lease_epoch", true)?;
    non_empty(value, "clock_revision")?;
    unsigned(value, "observed_at_tick", false)?;
    non_empty(value, "attempt_id")?;
    non_empty(value, "execution_id")?;
    digest(value, "execution_snapshot_digest")?;
    digest(value, "sandbox_profile_digest")?;
    non_empty(value, "safety_decision_id")
}

fn completed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "step_id",
            "attempt_id",
            "execution_id",
            "result_digest",
            "step_evidence",
            "criterion_evidence",
        ],
        &[],
    )?;
    terminal_attempt(value)?;
    digest(value, "result_digest")?;
    content(value, "step_evidence")?;
    content(value, "criterion_evidence")
}

fn failed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "step_id",
            "attempt_id",
            "execution_id",
            "reason",
            "retry_posture",
        ],
        &["evidence"],
    )?;
    terminal_attempt(value)?;
    non_empty(value, "reason")?;
    enumeration(
        value,
        "retry_posture",
        &["retry", "suspend", "replan", "fail"],
    )?;
    optional_content(value, "evidence")
}

fn suspended(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(
        value,
        &[
            "step_id",
            "attempt_id",
            "execution_id",
            "continuation_kind",
            "continuation_reference",
        ],
        &[],
    )?;
    terminal_attempt(value)?;
    continuation(value)
}

fn resumed(value: &Map<String, Value>) -> Result<(), LedgerError> {
    mutation_with(value, &["step_id", "resolved_continuation_reference"], &[])?;
    non_empty(value, "step_id")?;
    non_empty(value, "resolved_continuation_reference")
}

fn terminal_attempt(value: &Map<String, Value>) -> Result<(), LedgerError> {
    non_empty(value, "step_id")?;
    non_empty(value, "attempt_id")?;
    non_empty(value, "execution_id")
}
