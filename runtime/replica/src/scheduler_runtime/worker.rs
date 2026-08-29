use garive_ledger::SessionId;
use garive_scheduler::{
    next_occurrence, DueOccurrence, ScheduleDecision, ScheduleErrorCode, ScheduleIntent,
};

use crate::{
    plan_schedule_claimed, plan_schedule_exhausted, plan_schedule_failed, plan_schedule_fired,
    plan_schedule_skipped, ScheduleDispatchDisposition, ScheduleLease, ScheduleLeaseError,
    ScheduleLeaseRequest, ScheduleLifecycleContext, SqliteLedger, SqliteLedgerError,
};

use super::{reconstruct_schedule_state, ScheduleRuntimeState};

/// Explicit wall/monotonic clock observation used by one worker reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleClockReading {
    /// Canonical RFC 3339 UTC wall-clock value.
    pub observed_at_utc: String,
    /// Monotonic process clock in milliseconds for lease arithmetic.
    pub monotonic_ms: u64,
}

/// Clock port; Runtime composition supplies all time values explicitly.
pub trait ScheduleClock {
    /// Produces one coherent wall/monotonic observation.
    fn observe(&self) -> Result<ScheduleClockReading, ScheduleErrorCode>;
}

/// Mutation whose current authority must be revalidated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleAuthorityOperation {
    /// Create a new owned schedule.
    Create,
    /// Replace an active revision with another exact revision.
    Update,
    /// Cancel one exact active revision.
    Cancel,
    /// Submit or reconcile one deterministic C6 command.
    Dispatch,
    /// Persist one bounded skipped range.
    Skip,
    /// Persist a terminal policy/runtime failure.
    Fail,
    /// Persist deterministic recurrence exhaustion.
    Exhaust,
}

/// Current schedule ownership and Session/Agent authorization boundary.
pub trait ScheduleAuthorityPort {
    /// Revalidates current authority without relying on stored credentials.
    fn authorize(
        &self,
        session_id: &SessionId,
        intent: &ScheduleIntent,
        operation: ScheduleAuthorityOperation,
    ) -> Result<(), ScheduleErrorCode>;
}

/// Durable C6 command commit/replay receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleCommandReceipt {
    /// Exact deterministic command identity requested by Scheduler.
    pub runtime_command_id: String,
    /// Whether the command was newly committed or exactly replayed.
    pub disposition: ScheduleDispatchDisposition,
    /// Position of a durable C6 command fact carrying that identity.
    pub committed_position: u64,
}

/// Port resolving the intent's exact subject binding and submitting C6.
pub trait ScheduleCommandDispatcher {
    /// Finds a prior durable result without initiating new work.
    fn reconcile(
        &mut self,
        session_id: &SessionId,
        runtime_command_id: &str,
    ) -> Result<Option<ScheduleCommandReceipt>, ScheduleErrorCode>;

    /// Resolves and idempotently submits the exact scheduled command.
    fn submit(
        &mut self,
        session_id: &SessionId,
        intent: &ScheduleIntent,
        occurrence: &DueOccurrence,
    ) -> Result<ScheduleCommandReceipt, ScheduleErrorCode>;
}

/// Explicit worker and bounded lease configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleTickConfig {
    /// Stable worker/process identity.
    pub owner_id: String,
    /// Unique token for this tick/occurrence attempt.
    pub lease_id: String,
    /// Non-zero lease duration in milliseconds.
    pub lease_duration_ms: u64,
}

/// Observable result of one bounded schedule reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleTickOutcome {
    /// Active occurrence is not due yet.
    NotDue,
    /// Another worker retains the current occurrence lease.
    LeaseBusy,
    /// One exact C6 command was durably committed/replayed and recorded fired.
    Fired(ScheduleCommandReceipt),
    /// One bounded overdue range was durably skipped.
    Skipped {
        /// Inclusive first skipped ordinal.
        first_ordinal: u64,
        /// Inclusive last skipped ordinal.
        last_ordinal: u64,
    },
    /// Revision was durably disabled with this stable failure.
    Failed(ScheduleErrorCode),
    /// No recurrence remains and exhaustion was durably recorded.
    Exhausted,
    /// Revision was already cancelled, failed, or exhausted.
    Inactive,
}

/// Runs at most one durable schedule transition and one idempotent C6 submission.
pub fn run_schedule_once(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    schedule_id: &str,
    config: &ScheduleTickConfig,
    clock: &dyn ScheduleClock,
    authority: &dyn ScheduleAuthorityPort,
    dispatcher: &mut dyn ScheduleCommandDispatcher,
) -> Result<ScheduleTickOutcome, ScheduleErrorCode> {
    validate_config(config)?;
    let state = reconstruct_schedule_state(ledger, session_id, schedule_id)?;
    if !state.active {
        return Ok(ScheduleTickOutcome::Inactive);
    }
    let reading = clock.observe()?;
    let context = ScheduleLifecycleContext {
        recorded_at: reading.observed_at_utc.clone(),
    };
    if let Some(pending) = state.pending_claim.clone() {
        return dispatch_occurrence(
            ledger,
            session_id,
            state,
            pending.occurrence,
            true,
            config,
            &reading,
            &context,
            authority,
            dispatcher,
        );
    }
    match next_occurrence(
        &state.intent,
        state.last_handled_ordinal,
        &reading.observed_at_utc,
    )
    .map_err(|error| error.code())?
    {
        ScheduleDecision::NotDue(_) => Ok(ScheduleTickOutcome::NotDue),
        ScheduleDecision::Due(occurrence) => dispatch_occurrence(
            ledger, session_id, state, occurrence, false, config, &reading, &context, authority,
            dispatcher,
        ),
        ScheduleDecision::Skipped(skipped) => {
            if let Err(code) =
                authority.authorize(session_id, &state.intent, ScheduleAuthorityOperation::Skip)
            {
                return fail_unclaimed(ledger, session_id, &state, &context, code);
            }
            let fact =
                plan_schedule_skipped(&context, &state.intent, &skipped, &reading.observed_at_utc)
                    .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
            ledger
                .commit(session_id.clone(), state.session_version, vec![fact])
                .map_err(map_ledger)?;
            Ok(ScheduleTickOutcome::Skipped {
                first_ordinal: skipped.first_ordinal,
                last_ordinal: skipped.last_ordinal,
            })
        }
        ScheduleDecision::FailMisfire(occurrence) => fail_occurrence(
            ledger,
            session_id,
            &state,
            &occurrence,
            config,
            &reading,
            &context,
            ScheduleErrorCode::MisfireLimitExceeded,
            authority,
        ),
        ScheduleDecision::Exhausted => {
            authority.authorize(
                session_id,
                &state.intent,
                ScheduleAuthorityOperation::Exhaust,
            )?;
            let handled = state
                .last_handled_ordinal
                .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
            let fact = plan_schedule_exhausted(&context, &state.intent, handled)
                .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
            ledger
                .commit(session_id.clone(), state.session_version, vec![fact])
                .map_err(map_ledger)?;
            Ok(ScheduleTickOutcome::Exhausted)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_occurrence(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    state: ScheduleRuntimeState,
    occurrence: DueOccurrence,
    recovering: bool,
    config: &ScheduleTickConfig,
    reading: &ScheduleClockReading,
    context: &ScheduleLifecycleContext,
    authority: &dyn ScheduleAuthorityPort,
    dispatcher: &mut dyn ScheduleCommandDispatcher,
) -> Result<ScheduleTickOutcome, ScheduleErrorCode> {
    let Some(lease) = acquire(ledger, session_id, &state, &occurrence, config, reading)? else {
        return Ok(ScheduleTickOutcome::LeaseBusy);
    };
    if let Err(code) = authority.authorize(
        session_id,
        &state.intent,
        ScheduleAuthorityOperation::Dispatch,
    ) {
        return persist_failure(
            ledger,
            &lease,
            &state,
            &occurrence,
            reading.monotonic_ms,
            context,
            code,
        );
    }
    let pending_epoch = state.pending_claim.as_ref().map(|claim| claim.lease_epoch);
    if !recovering || pending_epoch != Some(lease.epoch) {
        let claimed = plan_schedule_claimed(
            context,
            &state.intent,
            &occurrence,
            &config.lease_id,
            lease.epoch,
            state.through_position,
        )
        .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
        ledger
            .commit_schedule_leased(
                &lease,
                reading.monotonic_ms,
                state.session_version,
                vec![claimed],
            )
            .map_err(map_ledger)?;
    }
    let dispatched = if recovering {
        match dispatcher.reconcile(session_id, &occurrence.runtime_command_id) {
            Ok(Some(receipt)) => Ok(receipt),
            Ok(None) => dispatcher.submit(session_id, &state.intent, &occurrence),
            Err(code) => Err(code),
        }
    } else {
        dispatcher.submit(session_id, &state.intent, &occurrence)
    };
    let receipt = match dispatched {
        Ok(receipt) => receipt,
        Err(code) => {
            return persist_failure(
                ledger,
                &lease,
                &state,
                &occurrence,
                reading.monotonic_ms,
                context,
                code,
            );
        }
    };
    if let Err(code) = validate_receipt(ledger, session_id, &occurrence, &receipt) {
        return persist_failure(
            ledger,
            &lease,
            &state,
            &occurrence,
            reading.monotonic_ms,
            context,
            code,
        );
    }
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
    let fired = plan_schedule_fired(
        context,
        &state.intent,
        &occurrence,
        receipt.disposition,
        receipt.committed_position,
    )
    .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
    ledger
        .commit_schedule_leased(
            &lease,
            reading.monotonic_ms,
            watermark.session_version,
            vec![fired],
        )
        .map_err(map_ledger)?;
    ledger.release_schedule_lease(&lease).map_err(map_lease)?;
    Ok(ScheduleTickOutcome::Fired(receipt))
}

#[allow(clippy::too_many_arguments)]
fn fail_occurrence(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    state: &ScheduleRuntimeState,
    occurrence: &DueOccurrence,
    config: &ScheduleTickConfig,
    reading: &ScheduleClockReading,
    context: &ScheduleLifecycleContext,
    reason: ScheduleErrorCode,
    authority: &dyn ScheduleAuthorityPort,
) -> Result<ScheduleTickOutcome, ScheduleErrorCode> {
    let Some(lease) = acquire(ledger, session_id, state, occurrence, config, reading)? else {
        return Ok(ScheduleTickOutcome::LeaseBusy);
    };
    let reason = authority
        .authorize(session_id, &state.intent, ScheduleAuthorityOperation::Fail)
        .err()
        .unwrap_or(reason);
    persist_failure(
        ledger,
        &lease,
        state,
        occurrence,
        reading.monotonic_ms,
        context,
        reason,
    )
}

fn fail_unclaimed(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    state: &ScheduleRuntimeState,
    context: &ScheduleLifecycleContext,
    reason: ScheduleErrorCode,
) -> Result<ScheduleTickOutcome, ScheduleErrorCode> {
    let failed = plan_schedule_failed(context, &state.intent, None, reason)
        .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
    ledger
        .commit(session_id.clone(), state.session_version, vec![failed])
        .map_err(map_ledger)?;
    Ok(ScheduleTickOutcome::Failed(reason))
}

fn persist_failure(
    ledger: &mut SqliteLedger,
    lease: &ScheduleLease,
    state: &ScheduleRuntimeState,
    occurrence: &DueOccurrence,
    monotonic_ms: u64,
    context: &ScheduleLifecycleContext,
    reason: ScheduleErrorCode,
) -> Result<ScheduleTickOutcome, ScheduleErrorCode> {
    let watermark = ledger
        .session_watermark(&lease.session_id)
        .map_err(map_ledger)?
        .ok_or(ScheduleErrorCode::CorruptScheduleState)?;
    let failed = plan_schedule_failed(context, &state.intent, Some(occurrence), reason)
        .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
    ledger
        .commit_schedule_leased(lease, monotonic_ms, watermark.session_version, vec![failed])
        .map_err(map_ledger)?;
    ledger.release_schedule_lease(lease).map_err(map_lease)?;
    Ok(ScheduleTickOutcome::Failed(reason))
}

fn acquire(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    state: &ScheduleRuntimeState,
    occurrence: &DueOccurrence,
    config: &ScheduleTickConfig,
    reading: &ScheduleClockReading,
) -> Result<Option<ScheduleLease>, ScheduleErrorCode> {
    let request = ScheduleLeaseRequest {
        session_id: session_id.clone(),
        schedule_id: state.intent.schedule_id().into(),
        revision_id: state.intent.revision_id().into(),
        occurrence_id: occurrence.occurrence_id.clone(),
        ordinal: occurrence.ordinal,
        owner_id: config.owner_id.clone(),
        lease_id: config.lease_id.clone(),
        now_ms: reading.monotonic_ms,
        duration_ms: config.lease_duration_ms,
    };
    match ledger.acquire_schedule_lease(&request) {
        Ok(lease) => Ok(Some(lease)),
        Err(ScheduleLeaseError::AlreadyHeld) => Ok(None),
        Err(error) => Err(map_lease(error)),
    }
}

fn validate_receipt(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    occurrence: &DueOccurrence,
    receipt: &ScheduleCommandReceipt,
) -> Result<(), ScheduleErrorCode> {
    if receipt.runtime_command_id != occurrence.runtime_command_id
        || receipt.committed_position == 0
    {
        return Err(ScheduleErrorCode::DispatchConflict);
    }
    let fact = ledger
        .read_facts(
            session_id,
            receipt.committed_position - 1,
            receipt.committed_position,
            None,
        )
        .map_err(map_ledger)?
        .into_iter()
        .next()
        .ok_or(ScheduleErrorCode::DispatchConflict)?;
    let value: serde_json::Value = serde_json::from_str(fact.payload.as_json())
        .map_err(|_| ScheduleErrorCode::CorruptScheduleState)?;
    if value.get("command_id").and_then(serde_json::Value::as_str)
        == Some(&receipt.runtime_command_id)
    {
        Ok(())
    } else {
        Err(ScheduleErrorCode::DispatchConflict)
    }
}

fn validate_config(value: &ScheduleTickConfig) -> Result<(), ScheduleErrorCode> {
    if value.owner_id.is_empty() || value.lease_id.is_empty() || value.lease_duration_ms == 0 {
        Err(ScheduleErrorCode::InvalidSchedule)
    } else {
        Ok(())
    }
}

fn map_lease(error: ScheduleLeaseError) -> ScheduleErrorCode {
    match error {
        ScheduleLeaseError::InvalidRequest => ScheduleErrorCode::InvalidSchedule,
        ScheduleLeaseError::AlreadyHeld | ScheduleLeaseError::LeaseLost => {
            ScheduleErrorCode::LeaseLost
        }
        ScheduleLeaseError::RevisionNotActive => ScheduleErrorCode::RevisionConflict,
        ScheduleLeaseError::FactBindingMismatch => ScheduleErrorCode::CorruptScheduleState,
        ScheduleLeaseError::Storage => ScheduleErrorCode::DurabilityFailure,
    }
}

fn map_ledger(error: SqliteLedgerError) -> ScheduleErrorCode {
    match error {
        SqliteLedgerError::Storage(_) => ScheduleErrorCode::DurabilityFailure,
        SqliteLedgerError::ScheduleLease(error) => map_lease(error),
        SqliteLedgerError::Domain(garive_ledger::LedgerError::ConcurrentModification) => {
            ScheduleErrorCode::LeaseLost
        }
        _ => ScheduleErrorCode::CorruptScheduleState,
    }
}
