use std::collections::BTreeMap;

use garive_ledger::DurableFact;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{ActivityProjectionLimits, HostActivity, InstalledActivityCatalogue, LiveHostError};

const API_VERSION: &str = "v1";

pub(crate) struct ActivityProjection {
    pub events: BTreeMap<u64, ProjectedActivityEvent>,
    pub by_turn: BTreeMap<String, Vec<HostActivity>>,
}

pub(crate) struct ProjectedActivityEvent {
    pub event: &'static str,
    pub activity: HostActivity,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Family {
    Effect,
    Interaction,
    Rejection,
}

struct State {
    family: Family,
    public: HostActivity,
    first_position: u64,
    receipt_seen: bool,
}

pub(crate) fn project_activities(
    facts: &[DurableFact],
    catalogue: &InstalledActivityCatalogue,
    limits: ActivityProjectionLimits,
) -> Result<ActivityProjection, LiveHostError> {
    let mut states: BTreeMap<(String, String, String), State> = BTreeMap::new();
    let mut events = BTreeMap::new();
    let mut activity_facts = 0usize;
    for fact in facts {
        fact.verify().map_err(|_| LiveHostError::CorruptState)?;
        if !is_activity_fact(fact.kind.as_str()) {
            continue;
        }
        activity_facts = activity_facts
            .checked_add(1)
            .ok_or(LiveHostError::CorruptState)?;
        if activity_facts > limits.max_activity_facts {
            return Err(LiveHostError::CorruptState);
        }
        let turn = fact
            .turn_id
            .as_ref()
            .ok_or(LiveHostError::CorruptState)?
            .as_str()
            .to_owned();
        let payload: Value = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| LiveHostError::CorruptState)?;
        if matches!(fact.kind.as_str(), "effect.receipt" | "effect.observation") {
            let id = tool_id(fact)?;
            let state = states
                .get_mut(&(turn, "tool".into(), id))
                .ok_or(LiveHostError::CorruptState)?;
            if state.family != Family::Effect {
                return Err(LiveHostError::CorruptState);
            }
            if fact.kind.as_str() == "effect.receipt" {
                if state.public.state != "running" || state.receipt_seen {
                    return Err(LiveHostError::CorruptState);
                }
                state.receipt_seen = true;
            } else if !state.public.terminal {
                return Err(LiveHostError::CorruptState);
            }
            continue;
        }
        let projected = match fact.kind.as_str() {
            "tool.preparation_rejected" => rejection(fact, &payload)?,
            "effect.prepared" => prepared(fact, &payload, catalogue)?,
            "interaction.requested" => interaction_requested(fact, &payload)?,
            kind => transition(fact, kind, &payload, &mut states)?,
        };
        let key = (
            turn.clone(),
            projected.activity.kind.clone(),
            projected.activity.activity_id.clone(),
        );
        if projected.activity.activity_id.len() > limits.max_activity_id_bytes
            || projected.activity.label_key.len() > limits.max_label_bytes
        {
            return Err(LiveHostError::CorruptState);
        }
        if matches!(
            fact.kind.as_str(),
            "tool.preparation_rejected" | "effect.prepared" | "interaction.requested"
        ) {
            if states.contains_key(&key) {
                return Err(LiveHostError::CorruptState);
            }
            let family = match fact.kind.as_str() {
                "effect.prepared" => Family::Effect,
                "interaction.requested" => Family::Interaction,
                _ => Family::Rejection,
            };
            states.insert(
                key,
                State {
                    family,
                    public: projected.activity.clone(),
                    first_position: fact.position,
                    receipt_seen: false,
                },
            );
        }
        if events.insert(fact.position, projected).is_some() {
            return Err(LiveHostError::CorruptState);
        }
    }

    let mut grouped: BTreeMap<String, Vec<(u64, HostActivity)>> = BTreeMap::new();
    for ((turn, _, _), state) in states {
        grouped
            .entry(turn)
            .or_default()
            .push((state.first_position, state.public));
    }
    let mut by_turn = BTreeMap::new();
    for (turn, mut values) in grouped {
        values.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.activity_id.cmp(&right.1.activity_id))
        });
        if values.len() > limits.max_activities_per_turn {
            return Err(LiveHostError::CorruptState);
        }
        let activities = values
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        if serde_json::to_vec(&activities)
            .map_err(|_| LiveHostError::CorruptState)?
            .len()
            > limits.max_encoded_bytes_per_turn
        {
            return Err(LiveHostError::CorruptState);
        }
        by_turn.insert(turn, activities);
    }
    Ok(ActivityProjection { events, by_turn })
}

fn prepared(
    fact: &DurableFact,
    payload: &Value,
    catalogue: &InstalledActivityCatalogue,
) -> Result<ProjectedActivityEvent, LiveHostError> {
    let name = text(payload, "tool_name")?;
    let revision = text(payload, "tool_revision")?;
    let label = catalogue
        .descriptors
        .iter()
        .find(|item| item.tool_name == name && item.tool_revision == revision)
        .ok_or(LiveHostError::CorruptState)?
        .label_key
        .clone();
    Ok(event(
        "agent.activity.prepared",
        tool_id(fact)?,
        "tool",
        label,
        "prepared",
        fact.position,
        false,
        None,
    ))
}

fn rejection(fact: &DurableFact, payload: &Value) -> Result<ProjectedActivityEvent, LiveHostError> {
    let activity_id = format!(
        "{:x}",
        Sha256::digest(format!("preparation-rejected-v1\n{}", fact.fact_id.as_str()).as_bytes())
    );
    Ok(event(
        "agent.activity.rejected",
        activity_id,
        "tool",
        "agent.activity.tool_rejected".into(),
        "failed",
        fact.position,
        true,
        Some(text(payload, "code")?.into()),
    ))
}

fn interaction_requested(
    fact: &DurableFact,
    payload: &Value,
) -> Result<ProjectedActivityEvent, LiveHostError> {
    let kind = text(payload, "kind")?;
    let label = match kind {
        "approval" => "agent.activity.approval",
        "external_input" => "agent.activity.external_input",
        _ => return Err(LiveHostError::CorruptState),
    };
    Ok(event(
        "agent.activity.input_requested",
        text(payload, "interaction_id")?.into(),
        "interaction",
        label.into(),
        "waiting_for_input",
        fact.position,
        false,
        None,
    ))
}

fn transition(
    fact: &DurableFact,
    kind: &str,
    payload: &Value,
    states: &mut BTreeMap<(String, String, String), State>,
) -> Result<ProjectedActivityEvent, LiveHostError> {
    let turn = fact.turn_id.as_ref().ok_or(LiveHostError::CorruptState)?;
    let (class, id) = if kind.starts_with("interaction.") {
        ("interaction", text(payload, "interaction_id")?.to_owned())
    } else {
        ("tool", tool_id(fact)?)
    };
    let key = (turn.as_str().to_owned(), class.into(), id);
    let state = states.get_mut(&key).ok_or(LiveHostError::CorruptState)?;
    if state.public.terminal || state.family == Family::Rejection {
        return Err(LiveHostError::CorruptState);
    }
    let (event_name, next, terminal, safe_code) = match kind {
        "interaction.resolved" if state.public.state == "waiting_for_input" => (
            "agent.activity.input_received",
            "input_received",
            true,
            None,
        ),
        "interaction.cancelled" if state.public.state == "waiting_for_input" => (
            "agent.activity.cancelled",
            "cancelled",
            true,
            Some(text(payload, "reason")?.to_owned()),
        ),
        "effect.authorized" if state.public.state == "prepared" => {
            ("agent.activity.authorized", "authorized", false, None)
        }
        "effect.denied" if matches!(state.public.state.as_str(), "prepared" | "authorized") => (
            "agent.activity.denied",
            "denied",
            true,
            Some(text(payload, "code")?.to_owned()),
        ),
        "effect.started" if matches!(state.public.state.as_str(), "prepared" | "authorized") => {
            ("agent.activity.started", "running", false, None)
        }
        "effect.completed" if state.public.state == "running" && state.receipt_seen => {
            ("agent.activity.completed", "completed", true, None)
        }
        "effect.failed" if state.public.state == "authorized" => (
            "agent.activity.failed",
            "failed",
            true,
            Some(text(payload, "code")?.to_owned()),
        ),
        "effect.failed" if state.public.state == "running" && state.receipt_seen => (
            "agent.activity.failed",
            "failed",
            true,
            Some(text(payload, "code")?.to_owned()),
        ),
        "effect.uncertain" if state.public.state == "running" => (
            "agent.activity.attention_required",
            "attention_required",
            false,
            Some(text(payload, "reason")?.to_owned()),
        ),
        "effect.reconciled" if state.public.state == "attention_required" => {
            let decision = text(payload, "decision")?;
            let (next, code) = match decision {
                "completed" => ("completed", "reconciled_completed"),
                "failed" => ("failed", "reconciled_failed"),
                _ => return Err(LiveHostError::CorruptState),
            };
            ("agent.activity.reconciled", next, true, Some(code.into()))
        }
        _ => return Err(LiveHostError::CorruptState),
    };
    state.public.state = next.into();
    state.public.source_position = fact.position;
    state.public.terminal = terminal;
    state.public.safe_code = safe_code;
    Ok(ProjectedActivityEvent {
        event: event_name,
        activity: state.public.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
fn event(
    name: &'static str,
    activity_id: String,
    kind: &str,
    label_key: String,
    state: &str,
    source_position: u64,
    terminal: bool,
    safe_code: Option<String>,
) -> ProjectedActivityEvent {
    ProjectedActivityEvent {
        event: name,
        activity: HostActivity {
            api_version: API_VERSION,
            activity_id,
            kind: kind.into(),
            label_key,
            state: state.into(),
            source_position,
            terminal,
            safe_code,
        },
    }
}

fn tool_id(fact: &DurableFact) -> Result<String, LiveHostError> {
    fact.tool_invocation_id
        .as_ref()
        .map(|value| value.as_str().to_owned())
        .ok_or(LiveHostError::CorruptState)
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, LiveHostError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(LiveHostError::CorruptState)
}

fn is_activity_fact(kind: &str) -> bool {
    matches!(
        kind,
        "tool.preparation_rejected"
            | "effect.prepared"
            | "interaction.requested"
            | "interaction.resolved"
            | "interaction.cancelled"
            | "effect.authorized"
            | "effect.denied"
            | "effect.started"
            | "effect.receipt"
            | "effect.completed"
            | "effect.failed"
            | "effect.uncertain"
            | "effect.reconciled"
            | "effect.observation"
    )
}
