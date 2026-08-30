use std::collections::BTreeMap;
use std::fmt::Write;

use garive_ledger::DurableFact;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::{
    activity_transition, HostActivityV1, HostReadLimits, LiveHostError,
    PublicToolActivityCatalogueV1,
};

pub(super) struct ActivityProjection {
    pub by_turn: BTreeMap<String, Vec<HostActivityV1>>,
    pub events: BTreeMap<u64, (&'static str, HostActivityV1)>,
}

pub(super) fn project(
    facts: &[DurableFact],
    catalogue: &PublicToolActivityCatalogueV1,
    limits: HostReadLimits,
) -> Result<ActivityProjection, LiveHostError> {
    let mut records = BTreeMap::<String, Record>::new();
    let mut events = BTreeMap::new();
    for fact in facts {
        let kind = fact.kind.as_str();
        if !is_activity_fact(kind) {
            continue;
        }
        fact.verify().map_err(|_| LiveHostError::CorruptState)?;
        if fact.schema_version != 1 {
            return Err(LiveHostError::CorruptState);
        }
        let turn_id = fact
            .turn_id
            .as_ref()
            .ok_or(LiveHostError::CorruptState)?
            .as_str()
            .to_owned();
        let payload = object(fact)?;
        let projected = match kind {
            "tool.preparation_rejected" => {
                let code = activity_transition::code(
                    text(&payload, "code")?,
                    &[
                        "invalid_tool_name",
                        "tool_not_admitted",
                        "invalid_arguments_json",
                        "arguments_schema_mismatch",
                        "non_canonical_value",
                    ],
                )?;
                let activity_id = rejection_id(fact.fact_id.as_str());
                if records.contains_key(&activity_id) {
                    return Err(LiveHostError::CorruptState);
                }
                let record = Record::new(
                    activity_id.clone(),
                    turn_id,
                    "tool",
                    "agent.activity.tool_rejected",
                    "failed",
                    fact.position,
                    Some(code),
                );
                records.insert(activity_id, record.clone());
                Some(("agent.activity.rejected", record.view))
            }
            "effect.prepared" => {
                let activity_id = tool_id(fact)?;
                if records.contains_key(&activity_id) {
                    return Err(LiveHostError::CorruptState);
                }
                let label = catalogue
                    .descriptors
                    .iter()
                    .find(|value| {
                        value.tool_name == text(&payload, "tool_name").unwrap_or_default()
                            && value.tool_revision
                                == text(&payload, "tool_revision").unwrap_or_default()
                    })
                    .ok_or(LiveHostError::CorruptState)?
                    .label_key
                    .clone();
                let record = Record::new(
                    activity_id.clone(),
                    turn_id,
                    "tool",
                    &label,
                    "prepared",
                    fact.position,
                    None,
                );
                records.insert(activity_id, record.clone());
                Some(("agent.activity.prepared", record.view))
            }
            "interaction.requested" => {
                let tool = tool_id(fact)?;
                let parent = records.get(&tool).ok_or(LiveHostError::CorruptState)?;
                if parent.turn_id != turn_id || parent.view.state != "prepared" {
                    return Err(LiveHostError::CorruptState);
                }
                let activity_id = text(&payload, "interaction_id")?.to_owned();
                if records.contains_key(&activity_id) {
                    return Err(LiveHostError::CorruptState);
                }
                let label = match text(&payload, "kind")? {
                    "approval" => "agent.activity.approval",
                    "external_input" => "agent.activity.external_input",
                    _ => return Err(LiveHostError::CorruptState),
                };
                let record = Record::new(
                    activity_id.clone(),
                    turn_id,
                    "interaction",
                    label,
                    "waiting_for_input",
                    fact.position,
                    None,
                );
                records.insert(activity_id, record.clone());
                Some(("agent.activity.input_requested", record.view))
            }
            "interaction.resolved" | "interaction.cancelled" => {
                let activity_id = text(&payload, "interaction_id")?;
                let record = records
                    .get_mut(activity_id)
                    .ok_or(LiveHostError::CorruptState)?;
                if record.view.kind != "interaction"
                    || record.view.state != "waiting_for_input"
                    || record.turn_id != turn_id
                {
                    return Err(LiveHostError::CorruptState);
                }
                let (event, state, code) = if kind == "interaction.resolved" {
                    ("agent.activity.input_received", "input_received", None)
                } else {
                    (
                        "agent.activity.cancelled",
                        "cancelled",
                        Some(activity_transition::code(
                            text(&payload, "reason")?,
                            &["user", "expired", "turn_cancelled", "operator"],
                        )?),
                    )
                };
                record.advance(state, fact.position, code)?;
                Some((event, record.view.clone()))
            }
            "effect.receipt" => {
                let record = effect_record(&mut records, fact, &turn_id)?;
                if record.view.state != "running" || record.receipt_seen {
                    return Err(LiveHostError::CorruptState);
                }
                record.receipt_seen = true;
                None
            }
            "effect.observation" => {
                effect_record(&mut records, fact, &turn_id)?;
                None
            }
            _ => {
                let record = effect_record(&mut records, fact, &turn_id)?;
                let (event, state, code) = activity_transition::effect(
                    kind,
                    &payload,
                    &record.view.state,
                    record.receipt_seen,
                )?;
                record.advance(state, fact.position, code)?;
                Some((event, record.view.clone()))
            }
        };
        if let Some((event, activity)) = projected {
            validate_public(&activity, limits)?;
            events.insert(fact.position, (event, activity));
        }
    }
    let mut by_turn = BTreeMap::<String, Vec<(u64, HostActivityV1)>>::new();
    for record in records.into_values() {
        by_turn
            .entry(record.turn_id)
            .or_default()
            .push((record.first_position, record.view));
    }
    let by_turn = by_turn
        .into_iter()
        .map(|(turn, mut values)| {
            values.sort_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.activity_id.cmp(&right.1.activity_id))
            });
            if values.len() > limits.max_activities_per_turn {
                return Err(LiveHostError::ReadBoundExceeded);
            }
            Ok((turn, values.into_iter().map(|(_, value)| value).collect()))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    if serde_json::to_vec(&by_turn)
        .map_err(|_| LiveHostError::CorruptState)?
        .len()
        > limits.max_activity_bytes
    {
        return Err(LiveHostError::ReadBoundExceeded);
    }
    Ok(ActivityProjection { by_turn, events })
}

fn effect_record<'a>(
    records: &'a mut BTreeMap<String, Record>,
    fact: &DurableFact,
    turn_id: &str,
) -> Result<&'a mut Record, LiveHostError> {
    let record = records
        .get_mut(&tool_id(fact)?)
        .ok_or(LiveHostError::CorruptState)?;
    if record.view.kind != "tool" || record.turn_id != turn_id {
        return Err(LiveHostError::CorruptState);
    }
    Ok(record)
}

fn validate_public(value: &HostActivityV1, limits: HostReadLimits) -> Result<(), LiveHostError> {
    if value.activity_id.is_empty()
        || value.activity_id.len() > limits.max_activity_id_bytes
        || value.label_key.is_empty()
        || value.label_key.len() > limits.max_activity_label_bytes
        || value.source_position == 0
    {
        Err(LiveHostError::ReadBoundExceeded)
    } else {
        Ok(())
    }
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

fn object(fact: &DurableFact) -> Result<Map<String, Value>, LiveHostError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(LiveHostError::CorruptState)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, LiveHostError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(LiveHostError::CorruptState)
}

fn tool_id(fact: &DurableFact) -> Result<String, LiveHostError> {
    fact.tool_invocation_id
        .as_ref()
        .map(|value| value.as_str().to_owned())
        .ok_or(LiveHostError::CorruptState)
}

fn rejection_id(fact_id: &str) -> String {
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(format!("preparation-rejected-v1\n{fact_id}").as_bytes()) {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

#[derive(Clone)]
struct Record {
    turn_id: String,
    first_position: u64,
    view: HostActivityV1,
    receipt_seen: bool,
}

impl Record {
    fn new(
        activity_id: String,
        turn_id: String,
        kind: &str,
        label_key: &str,
        state: &str,
        position: u64,
        safe_code: Option<String>,
    ) -> Self {
        Self {
            turn_id,
            first_position: position,
            view: HostActivityV1 {
                api_version: "v1",
                activity_id,
                kind: kind.to_owned(),
                label_key: label_key.to_owned(),
                state: state.to_owned(),
                source_position: position,
                terminal: terminal(state),
                safe_code,
            },
            receipt_seen: false,
        }
    }

    fn advance(
        &mut self,
        state: &str,
        position: u64,
        safe_code: Option<String>,
    ) -> Result<(), LiveHostError> {
        if position <= self.view.source_position {
            return Err(LiveHostError::CorruptState);
        }
        self.view.state = state.to_owned();
        self.view.source_position = position;
        self.view.terminal = terminal(state);
        self.view.safe_code = safe_code;
        Ok(())
    }
}

fn terminal(state: &str) -> bool {
    matches!(
        state,
        "input_received" | "completed" | "denied" | "failed" | "cancelled"
    )
}
