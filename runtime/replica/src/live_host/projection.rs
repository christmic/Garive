use garive_ledger::{CanonicalPayload, DurableFact};
use serde::Deserialize;

use super::{LiveHostError, LiveHostEvent};

const API_VERSION: &str = "v1";

pub(crate) fn project_fact(fact: &DurableFact) -> Result<Option<LiveHostEvent>, LiveHostError> {
    fact.verify().map_err(|_| LiveHostError::CorruptState)?;
    let (event, text, execution_id) = match fact.kind.as_str() {
        "session.opened" => {
            let payload: SessionOpened = decode(fact)?;
            if payload.command_id.is_empty()
                || payload.definition_id.is_empty()
                || payload.definition_revision.is_empty()
                || payload.agent_instance_id.is_empty()
                || !valid_digest(&payload.snapshot_digest)
            {
                return Err(LiveHostError::CorruptState);
            }
            ("session.created", String::new(), String::new())
        }
        "turn.started" => {
            let payload: Started = decode(fact)?;
            if payload.kind != "start" {
                return Ok(None);
            }
            ("turn.started", String::new(), String::new())
        }
        "turn.completed" => {
            let payload: Completed = decode(fact)?;
            let text = display_text(&payload.response)?;
            ("turn.completed", text, payload.execution_id)
        }
        "turn.suspended" => ("turn.suspended", String::new(), terminal_execution(fact)?),
        "turn.stopped" => ("turn.stopped", String::new(), terminal_execution(fact)?),
        "turn.failed" => ("turn.failed", String::new(), terminal_execution(fact)?),
        _ => return Ok(None),
    };
    Ok(Some(LiveHostEvent {
        api_version: API_VERSION,
        session_id: fact.session_id.as_str().to_owned(),
        position: fact.position,
        event: event.to_owned(),
        turn_id: fact
            .turn_id
            .as_ref()
            .map_or_else(String::new, |value| value.as_str().to_owned()),
        execution_id,
        text,
    }))
}

fn terminal_execution(fact: &DurableFact) -> Result<String, LiveHostError> {
    #[derive(Deserialize)]
    struct Terminal {
        execution_id: String,
    }
    let payload: Terminal = decode(fact)?;
    if payload.execution_id.is_empty() {
        return Err(LiveHostError::CorruptState);
    }
    Ok(payload.execution_id)
}

fn decode<T: for<'de> Deserialize<'de>>(fact: &DurableFact) -> Result<T, LiveHostError> {
    serde_json::from_str(fact.payload.as_json()).map_err(|_| LiveHostError::CorruptState)
}

pub(super) fn display_text(content: &Content) -> Result<String, LiveHostError> {
    let value =
        serde_json::from_str(&content.inline_utf8).map_err(|_| LiveHostError::CorruptState)?;
    let canonical =
        CanonicalPayload::from_value(&value).map_err(|_| LiveHostError::CorruptState)?;
    if canonical.as_json() != content.inline_utf8 || canonical.sha256() != content.digest {
        return Err(LiveHostError::CorruptState);
    }
    let items: Vec<ResponseItem> =
        serde_json::from_str(canonical.as_json()).map_err(|_| LiveHostError::CorruptState)?;
    Ok(items
        .into_iter()
        .filter_map(|item| match item.kind.as_str() {
            "text" | "refusal" => item.text,
            _ => None,
        })
        .collect())
}

#[derive(Deserialize)]
struct Started {
    kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionOpened {
    command_id: String,
    definition_id: String,
    definition_revision: String,
    snapshot_digest: String,
    agent_instance_id: String,
}

#[derive(Deserialize)]
struct Completed {
    execution_id: String,
    response: Content,
}

#[derive(Deserialize)]
pub(super) struct Content {
    digest: String,
    inline_utf8: String,
}

#[derive(Deserialize)]
struct ResponseItem {
    kind: String,
    text: Option<String>,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
