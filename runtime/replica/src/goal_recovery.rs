use garive_goal::{GoalDefinitionV1, GoalEvidenceV1, GoalSnapshot, GoalState, GoalTransition};
use garive_ledger::{DurableFact, SessionId};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

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
    let mut graph = reconstruct_goal_graph(ledger, session_id)?;
    graph.remove(goal_id).ok_or(GoalRuntimeError::NotFound)
}

pub(crate) fn reconstruct_goal_graph(
    ledger: &SqliteLedger,
    session_id: &SessionId,
) -> Result<BTreeMap<String, GoalRuntimeState>, GoalRuntimeError> {
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(GoalRuntimeError::NotFound)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut partial = BTreeMap::<String, (Option<GoalSnapshot>, u32)>::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.kind.as_str().starts_with("goal."))
    {
        let payload: Value = serde_json::from_str(fact.payload.as_json()).map_err(corrupt)?;
        let value = payload
            .as_object()
            .ok_or(GoalRuntimeError::RecoveryCorrupt)?;
        let goal_id = text(value, "goal_id")?;
        if goal_id.is_empty() || text(value, "command_id")? != fact.fact_id.as_str() {
            return Err(GoalRuntimeError::RecoveryCorrupt);
        }
        let (snapshot, attempt_number) = partial.entry(goal_id.into()).or_default();
        apply(snapshot, attempt_number, fact)?;
    }
    let mut graph = BTreeMap::new();
    for (goal_id, (snapshot, attempt_number)) in partial {
        graph.insert(
            goal_id,
            GoalRuntimeState {
                snapshot: snapshot.ok_or(GoalRuntimeError::RecoveryCorrupt)?,
                attempt_number,
                session_version: watermark.session_version,
                through_position: watermark.max_position,
            },
        );
    }
    validate_goal_graph(&graph).map_err(|_| GoalRuntimeError::RecoveryCorrupt)?;
    Ok(graph)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GoalGraphError {
    MissingParent,
    ScopeExceeded,
    Cycle,
}

pub(crate) fn validate_goal_graph(
    graph: &BTreeMap<String, GoalRuntimeState>,
) -> Result<(), GoalGraphError> {
    for (goal_id, state) in graph {
        if let Some(parent_id) = state.snapshot.definition().parent_goal_id() {
            let parent = graph
                .get(parent_id.as_str())
                .ok_or(GoalGraphError::MissingParent)?;
            state
                .snapshot
                .definition()
                .validate_child_of(parent.snapshot.definition())
                .map_err(|_| GoalGraphError::ScopeExceeded)?;
        }
        let mut visited = BTreeSet::new();
        let mut cursor = Some(goal_id.as_str());
        while let Some(current) = cursor {
            if !visited.insert(current) {
                return Err(GoalGraphError::Cycle);
            }
            cursor = graph
                .get(current)
                .and_then(|item| item.snapshot.definition().parent_goal_id())
                .map(|parent| parent.as_str());
        }
    }
    Ok(())
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
