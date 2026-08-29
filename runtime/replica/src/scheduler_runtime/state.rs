use garive_ledger::{DurableFact, SessionId};
use garive_scheduler::{
    next_occurrence, schedule_occurrence, DueOccurrence, ScheduleDecision, ScheduleErrorCode,
    ScheduleIntent, ScheduleIntentBinding,
};
use serde_json::Value;

use crate::SqliteLedger;

/// One durable claimed occurrence awaiting an exact C6 command result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingScheduleClaim {
    /// Reconstructed deterministic occurrence.
    pub occurrence: DueOccurrence,
    /// Last committed operational lease epoch.
    pub lease_epoch: u64,
}

/// Verified durable state of one schedule revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRuntimeState {
    /// Reconstructed immutable portable intent.
    pub intent: ScheduleIntent,
    /// Highest fired or skipped ordinal, absent before the first handling.
    pub last_handled_ordinal: Option<u64>,
    /// Claim that committed before C6 dispatch and lacks `schedule.fired`.
    pub pending_claim: Option<PendingScheduleClaim>,
    /// Whether the revision still admits work.
    pub active: bool,
    /// Current Session version after the reconstructed prefix.
    pub session_version: u64,
    /// Highest reconstructed durable position.
    pub through_position: u64,
}

/// Reconstructs and verifies one schedule from the Session's fixed durable prefix.
pub fn reconstruct_schedule_state(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    schedule_id: &str,
) -> Result<ScheduleRuntimeState, ScheduleErrorCode> {
    if schedule_id.is_empty() {
        return Err(ScheduleErrorCode::InvalidSchedule);
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(ScheduleErrorCode::ScheduleNotFound)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let mut state: Option<ScheduleRuntimeState> = None;
    for fact in facts.iter().filter(|fact| belongs(fact, schedule_id)) {
        apply_fact(&mut state, fact, &facts)?;
    }
    let mut state = state.ok_or(ScheduleErrorCode::ScheduleNotFound)?;
    state.session_version = watermark.session_version;
    state.through_position = watermark.max_position;
    Ok(state)
}

fn apply_fact(
    state: &mut Option<ScheduleRuntimeState>,
    fact: &DurableFact,
    facts: &[DurableFact],
) -> Result<(), ScheduleErrorCode> {
    let value = payload(fact)?;
    match fact.kind.as_str() {
        "schedule.created" => {
            let intent_value = value
                .get("intent")
                .and_then(Value::as_object)
                .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
            let binding = ScheduleIntentBinding {
                digest: text_object(intent_value, "digest")?.into(),
                inline_utf8: text_object(intent_value, "inline_utf8")?.into(),
            };
            let intent = ScheduleIntent::from_binding(
                text(&value, "schedule_id")?,
                text(&value, "revision_id")?,
                &binding,
            )
            .map_err(|error| error.code())?;
            if intent.intent_digest().map_err(|error| error.code())?
                != text(&value, "intent_digest")?
            {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            *state = Some(ScheduleRuntimeState {
                intent,
                last_handled_ordinal: None,
                pending_claim: None,
                active: true,
                session_version: 0,
                through_position: 0,
            });
        }
        "schedule.claimed" => {
            let current = active_revision(state, &value)?;
            if unsigned(&value, "through_position")? != fact.position.saturating_sub(1) {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            let occurrence = exact_next(&current.intent, current.last_handled_ordinal, &value)?;
            current.pending_claim = Some(PendingScheduleClaim {
                occurrence,
                lease_epoch: unsigned(&value, "lease_epoch")?,
            });
        }
        "schedule.fired" => {
            let current = active_revision(state, &value)?;
            let pending = current
                .pending_claim
                .take()
                .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
            if !matches_occurrence(&value, &pending.occurrence)
                || text(&value, "runtime_command_id")? != pending.occurrence.runtime_command_id
                || !receipt_exists(facts, fact, &value)?
            {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            current.last_handled_ordinal = Some(pending.occurrence.ordinal);
        }
        "schedule.skipped" => {
            let current = active_revision(state, &value)?;
            if current.pending_claim.is_some() {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            let decision = next_occurrence(
                &current.intent,
                current.last_handled_ordinal,
                text(&value, "observed_at_utc")?,
            )
            .map_err(|error| error.code())?;
            let ScheduleDecision::Skipped(skipped) = decision else {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            };
            if skipped.first_ordinal != unsigned(&value, "first_ordinal")?
                || skipped.last_ordinal != unsigned(&value, "last_ordinal")?
                || skipped.first_due_at_utc != text(&value, "first_due_at_utc")?
                || skipped.last_due_at_utc != text(&value, "last_due_at_utc")?
            {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            current.last_handled_ordinal = Some(skipped.last_ordinal);
        }
        "schedule.cancelled" => {
            let current = state
                .as_mut()
                .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
            if current.intent.revision_id() != text(&value, "expected_revision_id")? {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            current.active = false;
        }
        "schedule.failed" => {
            let current = active_revision(state, &value)?;
            if let Some(ordinal) = value.get("ordinal").and_then(Value::as_u64) {
                let occurrence = schedule_occurrence(&current.intent, ordinal)
                    .map_err(|error| error.code())?
                    .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
                let expected = current
                    .last_handled_ordinal
                    .map_or(Some(1), |value| value.checked_add(1));
                if !matches_occurrence(&value, &occurrence) || expected != Some(ordinal) {
                    return Err(ScheduleErrorCode::CorruptScheduleState);
                }
            }
            current.pending_claim = None;
            current.active = false;
        }
        "schedule.exhausted" => {
            let current = active_revision(state, &value)?;
            if current.pending_claim.is_some()
                || current.last_handled_ordinal != Some(unsigned(&value, "last_handled_ordinal")?)
                || next_occurrence(
                    &current.intent,
                    current.last_handled_ordinal,
                    &fact.recorded_at,
                )
                .map_err(|error| error.code())?
                    != ScheduleDecision::Exhausted
            {
                return Err(ScheduleErrorCode::CorruptScheduleState);
            }
            current.active = false;
        }
        _ => return Err(ScheduleErrorCode::CorruptScheduleState),
    }
    Ok(())
}

fn active_revision<'a>(
    state: &'a mut Option<ScheduleRuntimeState>,
    value: &Value,
) -> Result<&'a mut ScheduleRuntimeState, ScheduleErrorCode> {
    let current = state
        .as_mut()
        .filter(|state| state.active)
        .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
    if current.intent.revision_id() == text(value, "revision_id")? {
        Ok(current)
    } else {
        Err(ScheduleErrorCode::CorruptScheduleState)
    }
}

fn exact_next(
    intent: &ScheduleIntent,
    handled: Option<u64>,
    value: &Value,
) -> Result<DueOccurrence, ScheduleErrorCode> {
    let decision = next_occurrence(intent, handled, text(value, "due_at_utc")?)
        .map_err(|error| error.code())?;
    let ScheduleDecision::Due(occurrence) = decision else {
        return Err(ScheduleErrorCode::CorruptScheduleState);
    };
    if matches_occurrence(value, &occurrence) {
        Ok(occurrence)
    } else {
        Err(ScheduleErrorCode::CorruptScheduleState)
    }
}

fn receipt_exists(
    facts: &[DurableFact],
    fired: &DurableFact,
    value: &Value,
) -> Result<bool, ScheduleErrorCode> {
    let position = unsigned(value, "committed_position")?;
    let command = text(value, "runtime_command_id")?;
    Ok(position < fired.position
        && facts.iter().any(|fact| {
            fact.position == position
                && payload(fact).ok().and_then(|value| {
                    value
                        .get("command_id")
                        .and_then(Value::as_str)
                        .map(|actual| actual == command)
                }) == Some(true)
        }))
}

fn belongs(fact: &DurableFact, schedule_id: &str) -> bool {
    payload(fact).ok().and_then(|value| {
        value
            .get("schedule_id")
            .and_then(Value::as_str)
            .map(|actual| actual == schedule_id)
    }) == Some(true)
}

fn matches_occurrence(value: &Value, occurrence: &DueOccurrence) -> bool {
    value.get("occurrence_id").and_then(Value::as_str) == Some(&occurrence.occurrence_id)
        && value.get("ordinal").and_then(Value::as_u64) == Some(occurrence.ordinal)
}

fn payload(fact: &DurableFact) -> Result<Value, ScheduleErrorCode> {
    serde_json::from_str(fact.payload.as_json())
        .map_err(|_| ScheduleErrorCode::CorruptScheduleState)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, ScheduleErrorCode> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(ScheduleErrorCode::CorruptScheduleState)
}
fn text_object<'a>(
    value: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, ScheduleErrorCode> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(ScheduleErrorCode::CorruptScheduleState)
}
fn unsigned(value: &Value, key: &str) -> Result<u64, ScheduleErrorCode> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(ScheduleErrorCode::CorruptScheduleState)
}
fn map_ledger(error: crate::SqliteLedgerError) -> ScheduleErrorCode {
    match error {
        crate::SqliteLedgerError::Storage(_) => ScheduleErrorCode::DurabilityFailure,
        _ => ScheduleErrorCode::CorruptScheduleState,
    }
}
