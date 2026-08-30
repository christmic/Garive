use std::collections::BTreeMap;

use garive_ledger::{DurableFact, SessionId};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{
    projection,
    timeline_prompt::{self, Interaction},
    HostReadLimits, LiveHostError, SuspensionViewV1, TurnTimelineItemV1, TurnTimelinePageV1,
};

const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;

pub(super) fn project_timeline(
    session_id: &SessionId,
    observed_max_position: u64,
    session_version: u64,
    after_position: u64,
    limit: usize,
    facts: &[DurableFact],
    limits: HostReadLimits,
) -> Result<TurnTimelinePageV1, LiveHostError> {
    if limit == 0
        || limit > limits.max_timeline_items
        || facts.len() > limits.max_facts
        || observed_max_position == 0
        || observed_max_position > MAX_SAFE_JSON_INTEGER
        || session_version == 0
        || session_version > MAX_SAFE_JSON_INTEGER
        || after_position > observed_max_position
        || facts.len() as u64 != observed_max_position
    {
        return Err(LiveHostError::ReadBoundExceeded);
    }
    verify_prefix(session_id, facts)?;
    let interactions = timeline_prompt::interactions(facts, limits)?;
    let mut turns = BTreeMap::<String, Turn>::new();
    for fact in facts.iter().skip(1) {
        let Some(turn_id) = fact.turn_id.as_ref().map(|value| value.as_str()) else {
            if is_public_lifecycle(fact.kind.as_str()) {
                return Err(LiveHostError::CorruptState);
            }
            continue;
        };
        match fact.kind.as_str() {
            "turn.started" => start_or_continue(&mut turns, turn_id, fact)?,
            "turn.input" => admit_input(&mut turns, turn_id, fact, limits)?,
            "turn.suspended" => suspend(
                &mut turns,
                turn_id,
                fact,
                session_version,
                &interactions,
                limits,
            )?,
            "turn.completed" => complete(&mut turns, turn_id, fact, limits)?,
            "turn.stopped" => terminal(&mut turns, turn_id, fact, "stopped")?,
            "turn.failed" => terminal(&mut turns, turn_id, fact, "failed")?,
            _ => {}
        }
    }
    let mut items = turns
        .into_values()
        .map(Turn::finish)
        .collect::<Result<Vec<_>, _>>()?;
    items.retain(|item| item.latest_position > after_position);
    items.sort_by_key(|item| (item.latest_position, item.started_position));
    let has_more = items.len() > limit;
    items.truncate(limit);
    let scanned_through_position = if has_more {
        items
            .last()
            .map(|item| item.latest_position)
            .ok_or(LiveHostError::CorruptState)?
    } else {
        observed_max_position
    };
    Ok(TurnTimelinePageV1 {
        api_version: "v1",
        session_id: session_id.as_str().to_owned(),
        items,
        scanned_through_position,
        observed_max_position,
        has_more,
    })
}

fn verify_prefix(session_id: &SessionId, facts: &[DurableFact]) -> Result<(), LiveHostError> {
    for (index, fact) in facts.iter().enumerate() {
        fact.verify().map_err(|_| LiveHostError::CorruptState)?;
        if &fact.session_id != session_id || fact.position != index as u64 + 1 {
            return Err(LiveHostError::CorruptState);
        }
    }
    let opened = facts.first().ok_or(LiveHostError::CorruptState)?;
    if opened.kind.as_str() != "session.opened" || opened.schema_version != 1 {
        return Err(LiveHostError::CorruptState);
    }
    Ok(())
}

fn is_public_lifecycle(kind: &str) -> bool {
    matches!(
        kind,
        "turn.started"
            | "turn.input"
            | "turn.suspended"
            | "turn.completed"
            | "turn.stopped"
            | "turn.failed"
    )
}

fn start_or_continue(
    turns: &mut BTreeMap<String, Turn>,
    turn_id: &str,
    fact: &DurableFact,
) -> Result<(), LiveHostError> {
    require_v1(fact)?;
    let payload: Started = decode(fact)?;
    match payload.kind.as_str() {
        "start" if !turns.contains_key(turn_id) => {
            turns.insert(turn_id.to_owned(), Turn::new(turn_id, fact.position));
        }
        "continue" => {
            let turn = turns.get_mut(turn_id).ok_or(LiveHostError::CorruptState)?;
            if turn.state != "suspended"
                || payload.prior_suspension_id.as_deref()
                    != turn
                        .suspension
                        .as_ref()
                        .map(|value| value.suspension_id.as_str())
                || payload.expected_session_version.is_none()
            {
                return Err(LiveHostError::CorruptState);
            }
            turn.state = "running";
            turn.latest_position = fact.position;
            turn.suspension = None;
        }
        _ => return Err(LiveHostError::CorruptState),
    }
    Ok(())
}

fn admit_input(
    turns: &mut BTreeMap<String, Turn>,
    turn_id: &str,
    fact: &DurableFact,
    limits: HostReadLimits,
) -> Result<(), LiveHostError> {
    require_v1(fact)?;
    let payload: Input = decode(fact)?;
    let turn = turns.get_mut(turn_id).ok_or(LiveHostError::CorruptState)?;
    verify_text(&payload.content)?;
    if payload.input_kind == "trusted_user" {
        if turn.user_text.is_some() || payload.suspension_id.is_some() {
            return Err(LiveHostError::CorruptState);
        }
        let (text, truncated) = truncate(payload.content.inline_utf8, limits.max_user_text_bytes);
        turn.user_text = Some(text);
        turn.content_truncated |= truncated;
    } else if payload.input_kind != "trusted_system" && payload.suspension_id.is_none() {
        return Err(LiveHostError::CorruptState);
    }
    turn.latest_position = fact.position;
    Ok(())
}

fn suspend(
    turns: &mut BTreeMap<String, Turn>,
    turn_id: &str,
    fact: &DurableFact,
    session_version: u64,
    interactions: &BTreeMap<String, Interaction>,
    limits: HostReadLimits,
) -> Result<(), LiveHostError> {
    require_v1(fact)?;
    let payload: Suspended = decode(fact)?;
    let turn = running(turns, turn_id)?;
    let suspension = timeline_prompt::suspension_view(
        &payload.suspension_id,
        &payload.reason,
        session_version,
        interactions,
        limits,
    )?;
    turn.state = "suspended";
    turn.latest_position = fact.position;
    turn.suspension = Some(suspension);
    Ok(())
}

fn complete(
    turns: &mut BTreeMap<String, Turn>,
    turn_id: &str,
    fact: &DurableFact,
    limits: HostReadLimits,
) -> Result<(), LiveHostError> {
    require_v1(fact)?;
    let payload: Completed = decode(fact)?;
    let text = projection::display_text(&payload.response)?;
    let turn = running(turns, turn_id)?;
    let (text, truncated) = truncate(text, limits.max_completion_bytes);
    turn.state = "completed";
    turn.latest_position = fact.position;
    turn.completion_text = Some(text);
    turn.content_truncated |= truncated;
    Ok(())
}

fn terminal(
    turns: &mut BTreeMap<String, Turn>,
    turn_id: &str,
    fact: &DurableFact,
    state: &'static str,
) -> Result<(), LiveHostError> {
    require_v1(fact)?;
    let _: Terminal = decode(fact)?;
    let turn = running(turns, turn_id)?;
    turn.state = state;
    turn.latest_position = fact.position;
    Ok(())
}

fn running<'a>(
    turns: &'a mut BTreeMap<String, Turn>,
    turn_id: &str,
) -> Result<&'a mut Turn, LiveHostError> {
    let turn = turns.get_mut(turn_id).ok_or(LiveHostError::CorruptState)?;
    if turn.state != "running" {
        return Err(LiveHostError::CorruptState);
    }
    Ok(turn)
}

fn verify_text(content: &Content) -> Result<(), LiveHostError> {
    if digest(content.inline_utf8.as_bytes()) != content.digest {
        return Err(LiveHostError::CorruptState);
    }
    Ok(())
}

fn truncate(mut value: String, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value, false);
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    (value, true)
}

fn digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn require_v1(fact: &DurableFact) -> Result<(), LiveHostError> {
    if fact.schema_version == 1 {
        Ok(())
    } else {
        Err(LiveHostError::CorruptState)
    }
}

fn decode<T: for<'de> Deserialize<'de>>(fact: &DurableFact) -> Result<T, LiveHostError> {
    serde_json::from_str(fact.payload.as_json()).map_err(|_| LiveHostError::CorruptState)
}

struct Turn {
    turn_id: String,
    started_position: u64,
    latest_position: u64,
    state: &'static str,
    user_text: Option<String>,
    completion_text: Option<String>,
    suspension: Option<SuspensionViewV1>,
    content_truncated: bool,
}

impl Turn {
    fn new(turn_id: &str, position: u64) -> Self {
        Self {
            turn_id: turn_id.to_owned(),
            started_position: position,
            latest_position: position,
            state: "running",
            user_text: None,
            completion_text: None,
            suspension: None,
            content_truncated: false,
        }
    }

    fn finish(mut self) -> Result<TurnTimelineItemV1, LiveHostError> {
        let user = self.user_text.take().ok_or(LiveHostError::CorruptState)?;
        Ok(TurnTimelineItemV1 {
            turn_id: self.turn_id,
            started_position: self.started_position,
            latest_position: self.latest_position,
            state: self.state.to_owned(),
            user_text: user,
            completion_text: self.completion_text,
            suspension: self.suspension,
            content_truncated: self.content_truncated,
        })
    }
}

#[derive(Deserialize)]
struct Started {
    kind: String,
    prior_suspension_id: Option<String>,
    expected_session_version: Option<u64>,
}

#[derive(Deserialize)]
struct Input {
    input_kind: String,
    content: Content,
    suspension_id: Option<String>,
}

#[derive(Deserialize)]
struct Suspended {
    suspension_id: String,
    reason: String,
}

#[derive(Deserialize)]
struct Completed {
    response: projection::Content,
}

#[derive(Deserialize)]
struct Terminal {
    #[serde(rename = "execution_id")]
    _execution_id: String,
}

#[derive(Deserialize)]
struct Content {
    digest: String,
    inline_utf8: String,
}
