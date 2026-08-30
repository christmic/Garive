//! Strict F0 safety-decision and sandbox proof payload validation.

use serde_json::{Map, Value};

use crate::LedgerError;

use super::values::{digest, enumeration, fields, non_empty, optional_non_empty, EMPTY};

pub(super) fn validate(kind: &str, value: &Map<String, Value>) -> Result<(), LedgerError> {
    match kind {
        "safety.decided" => safety_decided(value),
        "sandbox.bound" => sandbox_bound(value),
        "sandbox.preflighted" => sandbox_preflighted(value),
        _ => Err(LedgerError::InvalidFact),
    }
}

fn safety_decided(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "request_id",
            "decision_id",
            "disposition",
            "prepared_digest",
            "tool_name",
            "tool_revision",
            "actor_authority_reference",
            "exact_access_digest",
            "sandbox_requirements_digest",
            "policy_revision",
        ],
        &[
            "goal_reference",
            "plan_reference",
            "constraints_digest",
            "safe_code",
        ],
    )?;
    identities(
        value,
        &[
            "request_id",
            "decision_id",
            "tool_name",
            "tool_revision",
            "actor_authority_reference",
            "policy_revision",
        ],
    )?;
    optional_non_empty(value, "goal_reference")?;
    optional_non_empty(value, "plan_reference")?;
    digests(
        value,
        &[
            "prepared_digest",
            "exact_access_digest",
            "sandbox_requirements_digest",
        ],
    )?;
    match enumeration(
        value,
        "disposition",
        &["allow", "deny", "interaction_required"],
    )? {
        "allow" if value.get("safe_code").is_none() => digest(value, "constraints_digest"),
        "deny" if value.get("constraints_digest").is_none() => exact_code(value, "safety_denied"),
        "interaction_required" if value.get("constraints_digest").is_none() => {
            exact_code(value, "safety_interaction_required")
        }
        _ => Err(LedgerError::InvalidFact),
    }
}

fn sandbox_bound(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "binding_id",
            "decision_id",
            "prepared_digest",
            "workspace_capability_id",
            "executor_id",
            "executor_revision",
            "policy_revision",
            "access_scope_digest",
            "enforcement_digest",
            "effective_limits_digest",
        ],
        EMPTY,
    )?;
    identities(
        value,
        &[
            "binding_id",
            "decision_id",
            "workspace_capability_id",
            "executor_id",
            "executor_revision",
            "policy_revision",
        ],
    )?;
    digests(
        value,
        &[
            "prepared_digest",
            "access_scope_digest",
            "enforcement_digest",
            "effective_limits_digest",
        ],
    )
}

fn sandbox_preflighted(value: &Map<String, Value>) -> Result<(), LedgerError> {
    fields(
        value,
        &[
            "preflight_id",
            "binding_id",
            "decision_id",
            "prepared_digest",
            "grant_id",
            "executor_id",
            "executor_revision",
            "dispatch_attempt_id",
        ],
        EMPTY,
    )?;
    identities(
        value,
        &[
            "preflight_id",
            "binding_id",
            "decision_id",
            "grant_id",
            "executor_id",
            "executor_revision",
            "dispatch_attempt_id",
        ],
    )?;
    digest(value, "prepared_digest")
}

fn identities(value: &Map<String, Value>, keys: &[&str]) -> Result<(), LedgerError> {
    keys.iter().try_for_each(|key| non_empty(value, key))
}

fn digests(value: &Map<String, Value>, keys: &[&str]) -> Result<(), LedgerError> {
    keys.iter().try_for_each(|key| digest(value, key))
}

fn exact_code(value: &Map<String, Value>, expected: &str) -> Result<(), LedgerError> {
    if value.get("safe_code").and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(LedgerError::InvalidFact)
    }
}
