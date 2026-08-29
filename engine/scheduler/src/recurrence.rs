use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde_json::json;

use crate::values::{canonical_digest, canonical_utc};
use crate::{MisfirePolicy, ScheduleError, ScheduleErrorCode, ScheduleIntent, ScheduleTiming};

const OCCURRENCE_CONTRACT: &str = "garive.schedule-occurrence";
const COMMAND_CONTRACT: &str = "garive.schedule-command";
const CONTRACT_VERSION: u32 = 1;
const OCCURRENCE_PREFIX: &str = "occurrence-";
const COMMAND_PREFIX: &str = "schedule-command-";

/// One exact due occurrence with deterministic durable identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DueOccurrence {
    /// Non-zero revision-local ordinal.
    pub ordinal: u64,
    /// Canonical declared due instant.
    pub due_at_utc: String,
    /// Deterministic occurrence identity.
    pub occurrence_id: String,
    /// Deterministic C6 command identity.
    pub runtime_command_id: String,
}

/// One bounded contiguous range produced by `MisfirePolicy::Skip`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkippedOccurrences {
    /// First skipped ordinal.
    pub first_ordinal: u64,
    /// Last skipped ordinal, inclusive.
    pub last_ordinal: u64,
    /// Declared due instant of the first skipped ordinal.
    pub first_due_at_utc: String,
    /// Declared due instant of the last skipped ordinal.
    pub last_due_at_utc: String,
    /// First unhandled due occurrence, or none when exhausted.
    pub next_due: Option<DueOccurrence>,
}

/// Pure deterministic reduction result for one observed clock value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleDecision {
    /// The next occurrence exists but is not due.
    NotDue(DueOccurrence),
    /// Exactly one occurrence may be claimed.
    Due(DueOccurrence),
    /// One skipped range must commit before reducing again.
    Skipped(SkippedOccurrences),
    /// No occurrence remains under the timing bound.
    Exhausted,
    /// Misfire policy requires terminal schedule failure.
    FailMisfire(DueOccurrence),
}

/// Reduces one immutable intent and durable handled prefix against an explicit clock.
pub fn next_occurrence(
    intent: &ScheduleIntent,
    last_handled_ordinal: Option<u64>,
    observed_now_utc: &str,
) -> Result<ScheduleDecision, ScheduleError> {
    let now = canonical_utc(observed_now_utc)
        .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::ClockInvalid))?;
    let ordinal = match last_handled_ordinal {
        Some(value) => value
            .checked_add(1)
            .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?,
        None => 1,
    };
    let Some(next) = schedule_occurrence(intent, ordinal)? else {
        return Ok(ScheduleDecision::Exhausted);
    };
    let due = canonical_utc(&next.due_at_utc)
        .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
    if now < due {
        return Ok(ScheduleDecision::NotDue(next));
    }
    let lateness = milliseconds(intent.max_lateness_ms())?;
    let latest = due
        .checked_add_signed(lateness)
        .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
    if now <= latest {
        return Ok(ScheduleDecision::Due(next));
    }
    match intent.misfire_policy() {
        MisfirePolicy::FireOnce => Ok(ScheduleDecision::Due(next)),
        MisfirePolicy::Fail => Ok(ScheduleDecision::FailMisfire(next)),
        MisfirePolicy::Skip => skip_overdue(intent, next, due, now, lateness),
    }
}

fn skip_overdue(
    intent: &ScheduleIntent,
    first: DueOccurrence,
    first_due: DateTime<Utc>,
    now: DateTime<Utc>,
    lateness: TimeDelta,
) -> Result<ScheduleDecision, ScheduleError> {
    let max_ordinal = match intent.timing() {
        ScheduleTiming::At { .. } => 1,
        ScheduleTiming::FixedDelay {
            max_occurrences, ..
        } => max_occurrences.unwrap_or(u64::MAX),
    };
    let last_ordinal = match intent.timing() {
        ScheduleTiming::At { .. } => 1,
        ScheduleTiming::FixedDelay { delay_ms, .. } => {
            let cutoff = now
                .checked_sub_signed(lateness)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            let delta_ns = cutoff
                .signed_duration_since(first_due)
                .num_nanoseconds()
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            let delay_ns_u64 = delay_ms
                .checked_mul(1_000_000)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            let delay_ns = i64::try_from(delay_ns_u64)
                .map_err(|_| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            let additional = u64::try_from((delta_ns - 1) / delay_ns)
                .map_err(|_| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            first
                .ordinal
                .checked_add(additional)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?
                .min(max_ordinal)
        }
    };
    let last = schedule_occurrence(intent, last_ordinal)?
        .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
    let next_due = if last_ordinal == max_ordinal {
        None
    } else {
        schedule_occurrence(
            intent,
            last_ordinal
                .checked_add(1)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?,
        )?
    };
    Ok(ScheduleDecision::Skipped(SkippedOccurrences {
        first_ordinal: first.ordinal,
        last_ordinal,
        first_due_at_utc: first.due_at_utc,
        last_due_at_utc: last.due_at_utc,
        next_due,
    }))
}

/// Derives one exact ordinal's due instant and deterministic identities.
pub fn schedule_occurrence(
    intent: &ScheduleIntent,
    ordinal: u64,
) -> Result<Option<DueOccurrence>, ScheduleError> {
    let due = match intent.timing() {
        ScheduleTiming::At { due_at_utc } => {
            if ordinal != 1 {
                return Ok(None);
            }
            canonical_utc(due_at_utc).unwrap()
        }
        ScheduleTiming::FixedDelay {
            first_due_at_utc,
            delay_ms,
            max_occurrences,
        } => {
            if max_occurrences.is_some_and(|max| ordinal > max) {
                return Ok(None);
            }
            let multiplier = ordinal
                .checked_sub(1)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            let offset = multiplier
                .checked_mul(*delay_ms)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?;
            canonical_utc(first_due_at_utc)
                .unwrap()
                .checked_add_signed(milliseconds(offset)?)
                .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))?
        }
    };
    let due_at_utc = due.to_rfc3339_opts(SecondsFormat::AutoSi, true);
    let semantic = json!({
        "version": CONTRACT_VERSION,
        "schedule_id": intent.schedule_id(),
        "revision_id": intent.revision_id(),
        "ordinal": ordinal,
        "due_at_utc": due_at_utc,
    });
    let occurrence_id = identity(OCCURRENCE_CONTRACT, OCCURRENCE_PREFIX, &semantic)?;
    let runtime_command_id = identity(COMMAND_CONTRACT, COMMAND_PREFIX, &semantic)?;
    Ok(Some(DueOccurrence {
        ordinal,
        due_at_utc,
        occurrence_id,
        runtime_command_id,
    }))
}

fn identity(
    contract: &str,
    prefix: &str,
    semantic: &serde_json::Value,
) -> Result<String, ScheduleError> {
    let mut value = semantic.clone();
    value
        .as_object_mut()
        .expect("semantic preimage is an object")
        .insert("contract".into(), json!(contract));
    canonical_digest(&value).map(|digest| format!("{prefix}{digest}"))
}

fn milliseconds(value: u64) -> Result<TimeDelta, ScheduleError> {
    i64::try_from(value)
        .ok()
        .and_then(TimeDelta::try_milliseconds)
        .ok_or_else(|| ScheduleError::new(ScheduleErrorCode::OccurrenceOverflow))
}
