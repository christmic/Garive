//! Startup recovery coordinator for local model-only Turn dispatches.

use garive_ledger::{DurableFact, TurnSnapshot};
use serde_json::Value;

use crate::{
    derive_runtime_recovery, plan_recovery_action_facts, plan_recovery_restart,
    select_runtime_recovery, CommittedTurn, EffectiveRuntimeLimits, RecoveryRestartCommand,
    RuntimeCommandId, RuntimeRecoveryAction, SqliteLedger,
};

/// Stable secret-free local restart failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRecoveryError {
    /// Explicit recovery limits or clock values are invalid.
    InvalidConfiguration,
    /// SQLite state could not be opened, verified or committed.
    DurabilityUnavailable,
    /// Durable positions cannot form an accepted recovery action.
    CorruptRecoveryState,
}
impl LocalRecoveryError {
    /// Returns the stable operational code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_composition",
            Self::DurabilityUnavailable => "durability_unavailable",
            Self::CorruptRecoveryState => "reconstruction_failed",
        }
    }
}

/// Applies C6 recovery to every local Session and returns replacement dispatches.
pub fn recover_local_dispatches(
    ledger: &mut SqliteLedger,
    max_recoveries: u64,
    recorded_at: &str,
) -> Result<Vec<CommittedTurn>, LocalRecoveryError> {
    if max_recoveries == 0 || !canonical_utc(recorded_at) {
        return Err(LocalRecoveryError::InvalidConfiguration);
    }
    let sessions = ledger
        .list_sessions()
        .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
    let mut output = Vec::new();
    for session_id in sessions {
        let turns = ledger
            .list_recoverable_turns(&session_id)
            .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
        for turn_id in turns {
            loop {
                let snapshot = ledger
                    .load_turn(&turn_id)
                    .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
                let recovery = derive_runtime_recovery(&snapshot, max_recoveries)
                    .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
                let action = select_runtime_recovery(recovery);
                match action {
                    RuntimeRecoveryAction::AbandonAndRestart => {
                        let plan = restart_plan(&snapshot, recorded_at)?;
                        let execution_id = plan
                            .execution_id
                            .clone()
                            .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
                        let committed = ledger
                            .commit(session_id.clone(), snapshot.session_version, plan.facts)
                            .map_err(|_| LocalRecoveryError::DurabilityUnavailable)?;
                        output.push(CommittedTurn {
                            session_id: session_id.clone(),
                            turn_id: plan.turn_id,
                            execution_id,
                            session_version: committed.session_version,
                            committed_position: *committed
                                .positions
                                .last()
                                .ok_or(LocalRecoveryError::CorruptRecoveryState)?,
                        });
                        break;
                    }
                    RuntimeRecoveryAction::ClassifyModelUncertain
                    | RuntimeRecoveryAction::ClassifyEffectUncertain
                    | RuntimeRecoveryAction::FailRecoveryBound => {
                        commit_action(ledger, &session_id, &snapshot, action, recorded_at)?;
                        break;
                    }
                    RuntimeRecoveryAction::RecoverReceiptTerminal => {
                        commit_action(ledger, &session_id, &snapshot, action, recorded_at)?;
                    }
                    RuntimeRecoveryAction::AwaitContinuation
                    | RuntimeRecoveryAction::ReturnCommittedTerminal => break,
                    RuntimeRecoveryAction::FailCorruptLedger => {
                        return Err(LocalRecoveryError::CorruptRecoveryState)
                    }
                }
            }
        }
    }
    Ok(output)
}

fn commit_action(
    ledger: &mut SqliteLedger,
    session_id: &garive_ledger::SessionId,
    snapshot: &TurnSnapshot,
    action: RuntimeRecoveryAction,
    recorded_at: &str,
) -> Result<(), LocalRecoveryError> {
    let facts = plan_recovery_action_facts(snapshot, action, recorded_at)
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?;
    ledger
        .commit(session_id.clone(), snapshot.session_version, facts)
        .map(|_| ())
        .map_err(|_| LocalRecoveryError::DurabilityUnavailable)
}

fn restart_plan(
    snapshot: &TurnSnapshot,
    recorded_at: &str,
) -> Result<crate::PlannedTurn, LocalRecoveryError> {
    let started = latest(snapshot, "execution.started")?;
    let execution_id = started
        .execution_id
        .clone()
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let value = payload(started)?;
    let recovery_ordinal = number(&value, "recovery_ordinal")?
        .checked_add(1)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let limits = value
        .get("limits")
        .and_then(Value::as_object)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)?;
    let command = RecoveryRestartCommand {
        recovery_id: RuntimeCommandId::new(format!(
            "local-recovery-{}-{recovery_ordinal}",
            snapshot
                .facts
                .first()
                .and_then(|fact| fact.turn_id.as_ref())
                .ok_or(LocalRecoveryError::CorruptRecoveryState)?
                .as_str()
        ))
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)?,
        turn_id: snapshot
            .facts
            .first()
            .and_then(|fact| fact.turn_id.clone())
            .ok_or(LocalRecoveryError::CorruptRecoveryState)?,
        lost_execution_id: execution_id,
        snapshot_digest: text(&value, "snapshot_digest")?.to_owned(),
        last_safe_position: snapshot.through_position,
        completed_iterations: number(&value, "completed_iterations")?,
        recovery_ordinal,
        limits: EffectiveRuntimeLimits {
            max_iterations: number_map(limits, "max_iterations")?,
            max_input_tokens: optional_number_map(limits, "max_input_tokens")?,
            max_output_tokens: optional_number_map(limits, "max_output_tokens")?,
            deadline_budget_ms: optional_number_map(limits, "deadline_budget_ms")?,
        },
        recorded_at: recorded_at.to_owned(),
    };
    plan_recovery_restart(&command).map_err(|_| LocalRecoveryError::CorruptRecoveryState)
}

fn latest<'a>(
    snapshot: &'a TurnSnapshot,
    kind: &str,
) -> Result<&'a DurableFact, LocalRecoveryError> {
    snapshot
        .facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == kind)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn payload(fact: &DurableFact) -> Result<Value, LocalRecoveryError> {
    serde_json::from_str(fact.payload.as_json())
        .map_err(|_| LocalRecoveryError::CorruptRecoveryState)
}

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, LocalRecoveryError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn number(value: &Value, name: &str) -> Result<u64, LocalRecoveryError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn number_map(
    value: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<u64, LocalRecoveryError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(LocalRecoveryError::CorruptRecoveryState)
}

fn optional_number_map(
    value: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Option<u64>, LocalRecoveryError> {
    value
        .get(name)
        .map(|value| {
            value
                .as_u64()
                .ok_or(LocalRecoveryError::CorruptRecoveryState)
        })
        .transpose()
}

fn canonical_utc(value: &str) -> bool {
    use chrono::{DateTime, SecondsFormat, Utc};
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}
