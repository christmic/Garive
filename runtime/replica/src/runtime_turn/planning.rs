use std::fmt::Write;

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, ToolInvocationId, TurnId,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::types::{
    CancelTurnCommand, ContinueTurnCommand, PlannedTurn, RuntimeCommandError, StartTurnCommand,
    SuspendedTurnState,
};

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

/// Creates the continuation transaction with a fresh disposable Execution.
pub fn plan_continue_turn(
    command: &ContinueTurnCommand,
    state: &SuspendedTurnState,
) -> Result<PlannedTurn, RuntimeCommandError> {
    validate_time(&command.recorded_at)?;
    validate_digest(&state.snapshot_digest)?;
    validate_digest(&state.trusted_input_digest)?;
    state.limits.validate()?;
    if command.expected_session_version != state.session_version {
        return Err(RuntimeCommandError::ConcurrentModification);
    }
    if command.turn_id != state.turn_id
        || command.expected_suspension_id != state.suspension_id
        || command.expected_suspension_id.is_empty()
    {
        return Err(RuntimeCommandError::ContinuationMismatch);
    }
    let seed = format!(
        "{}:{}",
        command.session_id.as_str(),
        command.command_id.as_str()
    );
    let execution_id = ExecutionId::try_from(
        format!(
            "execution-{}",
            digest(format!("{seed}:continue").as_bytes())
        )
        .as_str(),
    )
    .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let input_digest = digest(command.continuation_input.as_bytes());
    let mut facts = Vec::with_capacity(if command.interaction.is_some() { 4 } else { 3 });
    if let Some(interaction) = &command.interaction {
        validate_digest(&interaction.prepared_digest)?;
        facts.push(tool_fact(
            command.command_id.as_str(),
            &command.turn_id,
            &interaction.execution_id,
            &interaction.tool_invocation_id,
            json!({
                "interaction_id":interaction.interaction_id,
                "suspension_id":command.expected_suspension_id,
                "prepared_digest":interaction.prepared_digest,
                "response":{"digest":input_digest,"inline_utf8":command.continuation_input},
            }),
            &command.recorded_at,
        )?);
    }
    facts.extend([
        fact(
            command.command_id.as_str(),
            "turn.input",
            Some(&command.turn_id),
            None,
            json!({"input_kind":"continuation","content":{"digest":input_digest,"inline_utf8":command.continuation_input},"suspension_id":command.expected_suspension_id}),
            &command.recorded_at,
        )?,
        fact(
            command.command_id.as_str(),
            "turn.started",
            Some(&command.turn_id),
            None,
            json!({"command_id":command.command_id.as_str(),"kind":"continue","agent_instance_id":state.agent_instance_id.as_str(),"definition_id":state.definition_id.as_str(),"definition_revision":state.definition_revision.as_str(),"snapshot_digest":state.snapshot_digest,"trusted_input_digest":state.trusted_input_digest,"prior_suspension_id":state.suspension_id}),
            &command.recorded_at,
        )?,
        fact(
            command.command_id.as_str(),
            "execution.started",
            Some(&command.turn_id),
            Some(&execution_id),
            json!({"snapshot_digest":state.snapshot_digest,"through_position":state.through_position,"completed_iterations":state.completed_iterations,"limits":limits(&state.limits),"recovery_ordinal":state.recovery_ordinal}),
            &command.recorded_at,
        )?,
    ]);
    Ok(PlannedTurn {
        turn_id: command.turn_id.clone(),
        execution_id: Some(execution_id),
        facts,
    })
}

fn tool_fact(
    command_id: &str,
    turn_id: &TurnId,
    execution_id: &ExecutionId,
    tool_id: &ToolInvocationId,
    payload: Value,
    recorded_at: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    let mut output = fact(
        command_id,
        "interaction.resolved",
        Some(turn_id),
        Some(execution_id),
        payload,
        recorded_at,
    )?;
    output.tool_invocation_id = Some(tool_id.clone());
    Ok(output)
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
