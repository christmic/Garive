use std::fmt::Write;

use garive_ledger::{CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, TurnId};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::types::{CancelTurnCommand, PlannedTurn, RuntimeCommandError, StartTurnCommand};

/// Creates the exact three-fact StartTurn transaction with Runtime-owned identities.
pub fn plan_start_turn(
    command: &StartTurnCommand,
    through_position: u64,
) -> Result<PlannedTurn, RuntimeCommandError> {
    command.limits.validate()?;
    validate_digest(&command.snapshot_digest)?;
    validate_time(&command.recorded_at)?;
    let seed = format!(
        "{}:{}",
        command.session_id.as_str(),
        command.command_id.as_str()
    );
    let turn_id = TurnId::try_from(format!("turn-{}", digest(seed.as_bytes())).as_str())
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let execution_id = ExecutionId::try_from(
        format!("execution-{}", digest(format!("{seed}:start").as_bytes())).as_str(),
    )
    .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let input_digest = digest(command.trusted_input.as_bytes());
    let facts = vec![
        fact(
            command.command_id.as_str(),
            "turn.started",
            Some(&turn_id),
            None,
            json!({
                "command_id": command.command_id.as_str(), "kind": "start",
                "agent_instance_id": command.agent_instance_id.as_str(),
                "definition_id": command.definition_id.as_str(),
                "definition_revision": command.definition_revision.as_str(),
                "snapshot_digest": command.snapshot_digest,
                "trusted_input_digest": input_digest,
            }),
            &command.recorded_at,
        )?,
        fact(
            command.command_id.as_str(),
            "turn.input",
            Some(&turn_id),
            None,
            json!({"input_kind":"trusted_user","content":{"digest":input_digest,"inline_utf8":command.trusted_input}}),
            &command.recorded_at,
        )?,
        fact(
            command.command_id.as_str(),
            "execution.started",
            Some(&turn_id),
            Some(&execution_id),
            json!({
                "snapshot_digest":command.snapshot_digest,
                "through_position":through_position,
                "completed_iterations":0,
                "limits":limits(&command.limits),
                "recovery_ordinal":0,
            }),
            &command.recorded_at,
        )?,
    ];
    Ok(PlannedTurn {
        turn_id,
        execution_id: Some(execution_id),
        facts,
    })
}

/// Creates the one-fact durable cancellation-request transaction.
pub fn plan_cancel_turn(command: &CancelTurnCommand) -> Result<PlannedTurn, RuntimeCommandError> {
    validate_time(&command.recorded_at)?;
    let fact = fact(
        command.command_id.as_str(),
        "turn.cancel_requested",
        Some(&command.turn_id),
        None,
        json!({
            "command_id":command.command_id.as_str(),
            "reason":command.reason.as_str(),
            "requested_through_position":command.requested_through_position,
        }),
        &command.recorded_at,
    )?;
    Ok(PlannedTurn {
        turn_id: command.turn_id.clone(),
        execution_id: None,
        facts: vec![fact],
    })
}

fn fact(
    command_id: &str,
    kind: &str,
    turn_id: Option<&TurnId>,
    execution_id: Option<&ExecutionId>,
    payload: Value,
    recorded_at: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    let id = digest(format!("{command_id}:{kind}").as_bytes());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{id}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: turn_id.cloned(),
        execution_id: execution_id.cloned(),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        recorded_at: recorded_at.to_owned(),
    })
}

fn limits(value: &super::types::EffectiveRuntimeLimits) -> Value {
    let mut output = Map::from_iter([("max_iterations".into(), json!(value.max_iterations))]);
    for (key, value) in [
        ("max_input_tokens", value.max_input_tokens),
        ("max_output_tokens", value.max_output_tokens),
        ("deadline_budget_ms", value.deadline_budget_ms),
    ] {
        if let Some(value) = value {
            output.insert(key.into(), json!(value));
        }
    }
    Value::Object(output)
}

fn validate_digest(value: &str) -> Result<(), RuntimeCommandError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(RuntimeCommandError::InvalidCommand)
    }
}

fn validate_time(value: &str) -> Result<(), RuntimeCommandError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| RuntimeCommandError::InvalidCommand)
}

fn digest(value: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(value) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}
