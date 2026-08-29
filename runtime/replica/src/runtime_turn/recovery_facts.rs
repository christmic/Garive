use std::fmt::Write;

use garive_ledger::{CanonicalPayload, DurableFact, FactDraft, FactId, FactKind, TurnSnapshot};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::{RuntimeCommandError, RuntimeRecoveryAction};

/// Plans classification facts selected from one verified recovery prefix.
pub fn plan_recovery_action_facts(
    snapshot: &TurnSnapshot,
    action: RuntimeRecoveryAction,
    recorded_at: &str,
) -> Result<Vec<FactDraft>, RuntimeCommandError> {
    chrono::DateTime::parse_from_rfc3339(recorded_at)
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    match action {
        RuntimeRecoveryAction::ClassifyModelUncertain => {
            let source = latest(snapshot, "model.started")?;
            let payload = payload(source)?;
            Ok(vec![fact(
                source,
                "model.uncertain",
                json!({
                    "request_digest":text(&payload,"request_digest")?,"reason":"runtime_lost"
                }),
                recorded_at,
            )?])
        }
        RuntimeRecoveryAction::ClassifyEffectUncertain => {
            let source = latest(snapshot, "effect.started")?;
            let payload = payload(source)?;
            let uncertain = fact(
                source,
                "effect.uncertain",
                json!({
                    "prepared_digest":text(&payload,"prepared_digest")?,"reason":"executor_state_unknown"
                }),
                recorded_at,
            )?;
            let suspension_id = format!("suspension-{}", digest_text(source.fact_id.as_str()));
            let continuation = json!({"digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","inline_utf8":""});
            let usage = json!({"input_tokens":{"kind":"unknown"},"output_tokens":{"kind":"unknown"},"source":"estimated"});
            let mut execution = fact(
                source,
                "execution.suspended",
                json!({
                    "suspension_id":suspension_id,"reason":"operator_reconciliation",
                    "continuation":continuation,"usage":usage
                }),
                recorded_at,
            )?;
            execution.model_request_id = None;
            execution.tool_invocation_id = None;
            let execution_id = source
                .execution_id
                .as_ref()
                .ok_or(RuntimeCommandError::CorruptLedger)?;
            let mut turn = fact(
                source,
                "turn.suspended",
                json!({
                    "suspension_id":suspension_id,"execution_id":execution_id.as_str(),
                    "reason":"operator_reconciliation","continuation":continuation,"cumulative_usage":usage
                }),
                recorded_at,
            )?;
            turn.execution_id = None;
            turn.model_request_id = None;
            turn.tool_invocation_id = None;
            Ok(vec![uncertain, execution, turn])
        }
        RuntimeRecoveryAction::RecoverReceiptTerminal => recover_receipt(snapshot, recorded_at),
        RuntimeRecoveryAction::FailRecoveryBound => recovery_bound_terminal(snapshot, recorded_at),
        RuntimeRecoveryAction::AwaitContinuation
        | RuntimeRecoveryAction::ReturnCommittedTerminal => Ok(vec![]),
        RuntimeRecoveryAction::AbandonAndRestart => Err(RuntimeCommandError::InvalidCommand),
        RuntimeRecoveryAction::FailCorruptLedger => Err(RuntimeCommandError::CorruptLedger),
    }
}

fn recover_receipt(
    snapshot: &TurnSnapshot,
    recorded_at: &str,
) -> Result<Vec<FactDraft>, RuntimeCommandError> {
    let source = latest(snapshot, "effect.receipt")?;
    let payload = payload(source)?;
    let prepared = text(&payload, "prepared_digest")?;
    let receipt = text(&payload, "receipt_id")?;
    let evidence = payload
        .get("result_or_evidence")
        .cloned()
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let (kind, terminal) = match text(&payload, "classification")? {
        "completed" => (
            "effect.completed",
            json!({"prepared_digest":prepared,"receipt_id":receipt,"result":evidence}),
        ),
        "failed" => (
            "effect.failed",
            json!({"prepared_digest":prepared,"receipt_id":receipt,"code":"tool_failure","evidence":evidence}),
        ),
        _ => return Err(RuntimeCommandError::CorruptLedger),
    };
    Ok(vec![fact(source, kind, terminal, recorded_at)?])
}

fn recovery_bound_terminal(
    snapshot: &TurnSnapshot,
    recorded_at: &str,
) -> Result<Vec<FactDraft>, RuntimeCommandError> {
    let execution = latest(snapshot, "execution.started")?;
    let execution_id = execution
        .execution_id
        .as_ref()
        .ok_or(RuntimeCommandError::CorruptLedger)?;
    let usage = json!({"input_tokens":{"kind":"unknown"},"output_tokens":{"kind":"unknown"},"source":"estimated"});
    let execution_fact = fact(
        execution,
        "execution.failed",
        json!({
            "reason":"corrupt_recovery_state","usage":usage
        }),
        recorded_at,
    )?;
    let mut turn_fact = fact(
        execution,
        "turn.failed",
        json!({
            "execution_id":execution_id.as_str(),"reason":"corrupt_recovery_state","cumulative_usage":usage
        }),
        recorded_at,
    )?;
    turn_fact.execution_id = None;
    Ok(vec![execution_fact, turn_fact])
}

fn latest<'a>(
    snapshot: &'a TurnSnapshot,
    kind: &str,
) -> Result<&'a DurableFact, RuntimeCommandError> {
    snapshot
        .facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == kind)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn payload(fact: &DurableFact) -> Result<Map<String, Value>, RuntimeCommandError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, RuntimeCommandError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(RuntimeCommandError::CorruptLedger)
}

fn fact(
    source: &DurableFact,
    kind: &str,
    payload: Value,
    recorded_at: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    let mut id = String::with_capacity(64);
    for byte in Sha256::digest(format!("recovery:{}:{kind}", source.fact_id.as_str()).as_bytes()) {
        write!(&mut id, "{byte:02x}").unwrap();
    }
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{id}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: source.turn_id.clone(),
        execution_id: source.execution_id.clone(),
        model_request_id: source.model_request_id.clone(),
        tool_invocation_id: source.tool_invocation_id.clone(),
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: recorded_at.into(),
    })
}

fn digest_text(value: &str) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(value.as_bytes()) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}
