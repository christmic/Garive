use std::collections::BTreeMap;

use garive_ledger::{AgentInstanceId, DurableFact, SessionId};
use serde::Deserialize;

use super::{
    internal_turn::InternalPlannerTurns, HostReadLimits, InstalledAgent, LiveHostError,
    SessionSummaryV1, SessionViewV1,
};

const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;

pub(super) fn project_session(
    session_id: &SessionId,
    observed_max_position: u64,
    facts: &[DurableFact],
    installed: &InstalledAgent,
    limits: HostReadLimits,
) -> Result<SessionViewV1, LiveHostError> {
    if facts.len() > limits.max_facts
        || observed_max_position == 0
        || observed_max_position > MAX_SAFE_JSON_INTEGER
    {
        return Err(LiveHostError::ReadBoundExceeded);
    }
    verify_prefix(session_id, observed_max_position, facts)?;
    let opened = facts.first().ok_or(LiveHostError::CorruptState)?;
    if opened.position != 1
        || opened.kind.as_str() != "session.opened"
        || opened.schema_version != 1
    {
        return Err(LiveHostError::CorruptState);
    }
    let binding: SessionOpened = decode(opened)?;
    if binding.command_id.is_empty()
        || binding.definition_id != installed.definition_id
        || binding.definition_revision != installed.definition_revision
        || binding.snapshot_digest != installed.snapshot_digest
        || AgentInstanceId::try_from(binding.agent_instance_id.as_str()).is_err()
        || chrono::DateTime::parse_from_rfc3339(&opened.recorded_at).is_err()
    {
        return Err(LiveHostError::CorruptState);
    }

    let internal = InternalPlannerTurns::from_facts(facts)?;
    let mut turns: BTreeMap<String, TurnProjection> = BTreeMap::new();
    for fact in &facts[1..] {
        if internal.contains_fact(fact) {
            continue;
        }
        let kind = fact.kind.as_str();
        if !matches!(
            kind,
            "turn.started" | "turn.suspended" | "turn.completed" | "turn.stopped" | "turn.failed"
        ) {
            continue;
        }
        if fact.schema_version != 1 {
            return Err(LiveHostError::CorruptState);
        }
        let turn_id = fact
            .turn_id
            .as_ref()
            .ok_or(LiveHostError::CorruptState)?
            .as_str();
        if kind == "turn.started" {
            let started: TurnStarted = decode(fact)?;
            match started.kind.as_str() {
                "start"
                    if !turns.contains_key(turn_id)
                        && started.prior_suspension_id.is_none()
                        && started.expected_session_version.is_none() =>
                {
                    turns.insert(
                        turn_id.to_owned(),
                        TurnProjection {
                            started: fact.position,
                            state: "running",
                        },
                    );
                }
                "continue" => {
                    let turn = turns.get_mut(turn_id).ok_or(LiveHostError::CorruptState)?;
                    if turn.state != "suspended"
                        || started
                            .prior_suspension_id
                            .as_deref()
                            .is_none_or(str::is_empty)
                        || started.expected_session_version == Some(0)
                        || started.expected_session_version.is_none()
                    {
                        return Err(LiveHostError::CorruptState);
                    }
                    turn.state = "running";
                }
                _ => return Err(LiveHostError::CorruptState),
            }
        } else {
            let turn = turns.get_mut(turn_id).ok_or(LiveHostError::CorruptState)?;
            if turn.state != "running" {
                return Err(LiveHostError::CorruptState);
            }
            turn.state = match kind {
                "turn.suspended" => "suspended",
                "turn.completed" => "completed",
                "turn.stopped" => "stopped",
                "turn.failed" => "failed",
                _ => unreachable!(),
            };
        }
    }
    let latest = turns.iter().max_by_key(|(_, turn)| turn.started);
    let turn_count = u64::try_from(turns.len()).map_err(|_| LiveHostError::ReadBoundExceeded)?;
    let summary = SessionSummaryV1 {
        api_version: "v1",
        session_id: session_id.as_str().to_owned(),
        agent_instance_id: binding.agent_instance_id,
        definition_id: binding.definition_id,
        definition_revision: binding.definition_revision,
        opened_at: opened.recorded_at.clone(),
        latest_position: observed_max_position,
        latest_turn_id: latest.map(|(id, _)| id.clone()),
        latest_turn_state: latest.map(|(_, turn)| turn.state.to_owned()),
        turn_count,
    };
    Ok(SessionViewV1 {
        api_version: "v1",
        session: summary,
        observed_max_position,
    })
}

fn verify_prefix(
    session_id: &SessionId,
    observed_max_position: u64,
    facts: &[DurableFact],
) -> Result<(), LiveHostError> {
    let mut previous = 0;
    for fact in facts {
        fact.verify().map_err(|_| LiveHostError::CorruptState)?;
        if &fact.session_id != session_id
            || fact.position <= previous
            || fact.position > observed_max_position
        {
            return Err(LiveHostError::CorruptState);
        }
        previous = fact.position;
    }
    if previous != observed_max_position {
        return Err(LiveHostError::CorruptState);
    }
    Ok(())
}

fn decode<T: for<'de> Deserialize<'de>>(fact: &DurableFact) -> Result<T, LiveHostError> {
    fact.verify().map_err(|_| LiveHostError::CorruptState)?;
    serde_json::from_str(fact.payload.as_json()).map_err(|_| LiveHostError::CorruptState)
}

struct TurnProjection {
    started: u64,
    state: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionOpened {
    command_id: String,
    definition_id: String,
    definition_revision: String,
    snapshot_digest: String,
    agent_instance_id: String,
    #[serde(default, rename = "agent_name")]
    _agent_name: Option<String>,
}

#[derive(Deserialize)]
struct TurnStarted {
    kind: String,
    prior_suspension_id: Option<String>,
    expected_session_version: Option<u64>,
}
