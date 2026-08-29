use chrono::{DateTime, SecondsFormat, Utc};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind};
use garive_scheduler::{DueOccurrence, ScheduleErrorCode, ScheduleIntent, SkippedOccurrences};
use serde_json::{json, Map, Value};

use crate::RuntimeCommandError;

use super::encoding::digest;

/// Runtime-owned observation time for one portable schedule fact.
pub struct ScheduleLifecycleContext {
    /// Canonical RFC 3339 UTC time supplied by the configured clock port.
    pub recorded_at: String,
}

/// Stable cancellation reason persisted by Q0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleCancelReason {
    /// Requested by the owning user.
    User,
    /// Requested by an authorized operator.
    Operator,
    /// Required by current policy.
    Policy,
    /// Atomically replaced by another immutable revision.
    Superseded,
}

/// Exact result of submitting the deterministic C6 command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleDispatchDisposition {
    /// The command was newly committed.
    Committed,
    /// The exact prior command commit was replayed.
    Replayed,
}

/// Plans one immutable `schedule.created` fact.
pub fn plan_schedule_created(
    context: &ScheduleLifecycleContext,
    command_id: &str,
    intent: &ScheduleIntent,
) -> Result<FactDraft, RuntimeCommandError> {
    non_empty(command_id)?;
    let binding = intent
        .intent_binding()
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    schedule_fact(
        context,
        intent.schedule_id(),
        "schedule.created",
        command_id,
        json!({
            "command_id":command_id,
            "schedule_id":intent.schedule_id(),
            "revision_id":intent.revision_id(),
            "intent":{"digest":binding.digest,"inline_utf8":binding.inline_utf8},
            "intent_digest":binding.digest,
        }),
    )
}

/// Plans the durable claim that must commit before C6 dispatch.
#[allow(clippy::too_many_arguments)]
pub fn plan_schedule_claimed(
    context: &ScheduleLifecycleContext,
    intent: &ScheduleIntent,
    occurrence: &DueOccurrence,
    lease_id: &str,
    lease_epoch: u64,
    through_position: u64,
) -> Result<FactDraft, RuntimeCommandError> {
    non_empty(lease_id)?;
    if lease_epoch == 0 {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    schedule_fact(
        context,
        intent.schedule_id(),
        "schedule.claimed",
        &format!("{}:{lease_epoch}", occurrence.occurrence_id),
        json!({
            "schedule_id":intent.schedule_id(),"revision_id":intent.revision_id(),
            "occurrence_id":occurrence.occurrence_id,"ordinal":occurrence.ordinal,
            "due_at_utc":occurrence.due_at_utc,"lease_id":lease_id,
            "lease_epoch":lease_epoch,"through_position":through_position,
        }),
    )
}

/// Plans `schedule.fired` after the exact C6 command commit/replay result exists.
pub fn plan_schedule_fired(
    context: &ScheduleLifecycleContext,
    intent: &ScheduleIntent,
    occurrence: &DueOccurrence,
    disposition: ScheduleDispatchDisposition,
    committed_position: u64,
) -> Result<FactDraft, RuntimeCommandError> {
    if committed_position == 0 {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    schedule_fact(
        context,
        intent.schedule_id(),
        "schedule.fired",
        &occurrence.occurrence_id,
        json!({
            "schedule_id":intent.schedule_id(),"revision_id":intent.revision_id(),
            "occurrence_id":occurrence.occurrence_id,"ordinal":occurrence.ordinal,
            "runtime_command_id":occurrence.runtime_command_id,
            "disposition":disposition_name(disposition),"committed_position":committed_position,
        }),
    )
}

/// Plans one bounded contiguous `schedule.skipped` range.
pub fn plan_schedule_skipped(
    context: &ScheduleLifecycleContext,
    intent: &ScheduleIntent,
    skipped: &SkippedOccurrences,
    observed_at_utc: &str,
) -> Result<FactDraft, RuntimeCommandError> {
    canonical_utc(observed_at_utc)?;
    schedule_fact(
        context,
        intent.schedule_id(),
        "schedule.skipped",
        &format!("{}:{}", skipped.first_ordinal, skipped.last_ordinal),
        json!({
            "schedule_id":intent.schedule_id(),"revision_id":intent.revision_id(),
            "first_ordinal":skipped.first_ordinal,"last_ordinal":skipped.last_ordinal,
            "first_due_at_utc":skipped.first_due_at_utc,
            "last_due_at_utc":skipped.last_due_at_utc,"observed_at_utc":observed_at_utc,
        }),
    )
}

/// Plans a revision-checked cancellation command.
pub fn plan_schedule_cancelled(
    context: &ScheduleLifecycleContext,
    command_id: &str,
    schedule_id: &str,
    expected_revision_id: &str,
    reason: ScheduleCancelReason,
) -> Result<FactDraft, RuntimeCommandError> {
    non_empty(command_id)?;
    non_empty(schedule_id)?;
    non_empty(expected_revision_id)?;
    schedule_fact(
        context,
        schedule_id,
        "schedule.cancelled",
        command_id,
        json!({
            "command_id":command_id,"schedule_id":schedule_id,
            "expected_revision_id":expected_revision_id,"reason":cancel_reason(reason),
        }),
    )
}

/// Plans one terminal schedule failure with an optional exact occurrence.
pub fn plan_schedule_failed(
    context: &ScheduleLifecycleContext,
    intent: &ScheduleIntent,
    occurrence: Option<&DueOccurrence>,
    reason: ScheduleErrorCode,
) -> Result<FactDraft, RuntimeCommandError> {
    let mut payload = Map::from_iter([
        ("schedule_id".into(), json!(intent.schedule_id())),
        ("revision_id".into(), json!(intent.revision_id())),
        ("reason".into(), json!(reason.wire_name())),
    ]);
    if let Some(value) = occurrence {
        payload.insert("occurrence_id".into(), json!(value.occurrence_id));
        payload.insert("ordinal".into(), json!(value.ordinal));
    }
    schedule_fact(
        context,
        intent.schedule_id(),
        "schedule.failed",
        occurrence.map_or(reason.wire_name(), |value| &value.occurrence_id),
        Value::Object(payload),
    )
}

/// Plans the terminal fact proving no occurrence remains after a handled prefix.
pub fn plan_schedule_exhausted(
    context: &ScheduleLifecycleContext,
    intent: &ScheduleIntent,
    last_handled_ordinal: u64,
) -> Result<FactDraft, RuntimeCommandError> {
    if last_handled_ordinal == 0 {
        return Err(RuntimeCommandError::InvalidCommand);
    }
    schedule_fact(
        context,
        intent.schedule_id(),
        "schedule.exhausted",
        &last_handled_ordinal.to_string(),
        json!({
            "schedule_id":intent.schedule_id(),"revision_id":intent.revision_id(),
            "last_handled_ordinal":last_handled_ordinal,
        }),
    )
}

fn schedule_fact(
    context: &ScheduleLifecycleContext,
    schedule_id: &str,
    kind: &str,
    discriminator: &str,
    payload: Value,
) -> Result<FactDraft, RuntimeCommandError> {
    canonical_utc(&context.recorded_at)?;
    let id = digest(format!("{schedule_id}:{kind}:{discriminator}").as_bytes());
    Ok(FactDraft {
        fact_id: FactId::try_from(format!("fact-{id}").as_str())
            .map_err(|_| RuntimeCommandError::InvalidCommand)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).map_err(|_| RuntimeCommandError::InvalidCommand)?,
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload)
            .map_err(|_| RuntimeCommandError::InvariantViolation)?,
        recorded_at: context.recorded_at.clone(),
    })
}

fn canonical_utc(value: &str) -> Result<(), RuntimeCommandError> {
    if DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    }) {
        Ok(())
    } else {
        Err(RuntimeCommandError::InvalidCommand)
    }
}

fn non_empty(value: &str) -> Result<(), RuntimeCommandError> {
    if value.is_empty() {
        Err(RuntimeCommandError::InvalidCommand)
    } else {
        Ok(())
    }
}

const fn disposition_name(value: ScheduleDispatchDisposition) -> &'static str {
    match value {
        ScheduleDispatchDisposition::Committed => "committed",
        ScheduleDispatchDisposition::Replayed => "replayed",
    }
}

const fn cancel_reason(value: ScheduleCancelReason) -> &'static str {
    match value {
        ScheduleCancelReason::User => "user",
        ScheduleCancelReason::Operator => "operator",
        ScheduleCancelReason::Policy => "policy",
        ScheduleCancelReason::Superseded => "superseded",
    }
}
