use garive_goal::{GoalDefinitionV1, GoalEvidenceV1, GoalSnapshot, GoalState, GoalTransition};
use garive_ledger::{DurableFact, SessionId};
use serde_json::{Map, Value};

use crate::{GoalRuntimeError, GoalRuntimeState, SqliteLedger, SqliteLedgerError};

/// Reconstructs one Goal from a verified fixed Session prefix.
pub fn reconstruct_goal(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
) -> Result<GoalRuntimeState, GoalRuntimeError> {
    if goal_id.is_empty() {
        return Err(GoalRuntimeError::Invalid);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(GoalRuntimeError::NotFound)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut snapshot = None;
    let mut attempt_number = 0;
    for fact in facts.iter().filter(|fact| belongs(fact, goal_id)) {
        apply(&mut snapshot, &mut attempt_number, fact)?;
    }
    Ok(GoalRuntimeState {
        snapshot: snapshot.ok_or(GoalRuntimeError::NotFound)?,
        attempt_number,
        session_version: watermark.session_version,
        through_position: watermark.max_position,
    })
}

fn belongs(fact: &DurableFact, goal_id: &str) -> bool {
    if !fact.kind.as_str().starts_with("goal.") {
        return false;
    }
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.get("goal_id")?.as_str().map(str::to_owned))
        .is_some_and(|value| value == goal_id)
}

fn apply(
    snapshot: &mut Option<GoalSnapshot>,
    attempt_number: &mut u32,
    fact: &DurableFact,
) -> Result<(), GoalRuntimeError> {
    let payload: Value = serde_json::from_str(fact.payload.as_json()).map_err(corrupt)?;
    let value = payload
        .as_object()
        .ok_or(GoalRuntimeError::RecoveryCorrupt)?;
    if fact.kind.as_str() == "goal.created" {
        if snapshot.is_some() || unsigned(value, "revision")? != 1 {
            return Err(GoalRuntimeError::RecoveryCorrupt);
        }
        let definition = definition(value)?;
        if definition.goal_id().as_str() != text(value, "goal_id")? {
            return Err(GoalRuntimeError::RecoveryCorrupt);
        }
        *snapshot = Some(GoalSnapshot::new(definition));
        return Ok(());
    }
    let current = snapshot.as_ref().ok_or(GoalRuntimeError::RecoveryCorrupt)?;
    let expected = current.revision();
    let next_revision = expected
        .checked_add(1)
        .ok_or(GoalRuntimeError::RecoveryCorrupt)?;
    if unsigned(value, "revision")? != next_revision {
        return Err(GoalRuntimeError::RecoveryCorrupt);
    }
    let transition = match fact.kind.as_str() {
        "goal.revised" => {
            if unsigned(value, "previous_revision")? != expected
                || text(value, "previous_definition_digest")?
                    != current.definition().digest().map_err(corrupt)?
            {
                return Err(GoalRuntimeError::RecoveryCorrupt);
            }
            GoalTransition::Revise(Box::new(definition(value)?))
        }
        "goal.activated" => {
            let declared = u32::try_from(unsigned(value, "attempt_number")?)
                .map_err(|_| GoalRuntimeError::RecoveryCorrupt)?;
            let expected_attempt = match current.state() {
                GoalState::Draft => attempt_number
                    .checked_add(1)
                    .ok_or(GoalRuntimeError::RecoveryCorrupt)?,
                GoalState::Suspended => *attempt_number,
                _ => return Err(GoalRuntimeError::RecoveryCorrupt),
            };
            if declared != expected_attempt {
                return Err(GoalRuntimeError::RecoveryCorrupt);
            }
            *attempt_number = declared;
            GoalTransition::Activate
        }
        "goal.suspended" => GoalTransition::Suspend(text(value, "reason")?.into()),
        "goal.succeeded" => GoalTransition::Succeed(evidence(value, "evidence")?),
        "goal.failed" => {
            if value.contains_key("evidence") {
                evidence(value, "evidence")?;
            }
            GoalTransition::Fail(text(value, "code")?.into())
        }
        "goal.cancelled" => GoalTransition::Cancel(text(value, "reason")?.into()),
        _ => return Err(GoalRuntimeError::RecoveryCorrupt),
    };
    *snapshot = Some(current.apply(expected, transition).map_err(corrupt)?);
    Ok(())
}

fn definition(value: &Map<String, Value>) -> Result<GoalDefinitionV1, GoalRuntimeError> {
    let json = inline(value, "definition")?;
    let definition = GoalDefinitionV1::from_canonical_json(json).map_err(corrupt)?;
    if definition.digest().map_err(corrupt)? != text(value, "definition_digest")? {
        return Err(GoalRuntimeError::RecoveryCorrupt);
    }
    Ok(definition)
}

fn evidence(
    value: &Map<String, Value>,
    key: &str,
) -> Result<Vec<GoalEvidenceV1>, GoalRuntimeError> {
    GoalEvidenceV1::list_from_canonical_json(inline(value, key)?).map_err(corrupt)
}

fn inline<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, GoalRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("inline_utf8"))
        .and_then(Value::as_str)
        .ok_or(GoalRuntimeError::RecoveryCorrupt)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, GoalRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(GoalRuntimeError::RecoveryCorrupt)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, GoalRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(GoalRuntimeError::RecoveryCorrupt)
}

fn corrupt<T>(_: T) -> GoalRuntimeError {
    GoalRuntimeError::RecoveryCorrupt
}

fn map_ledger(error: SqliteLedgerError) -> GoalRuntimeError {
    match error {
        SqliteLedgerError::Storage(_) => GoalRuntimeError::DurabilityFailure,
        _ => GoalRuntimeError::RecoveryCorrupt,
    }
}
