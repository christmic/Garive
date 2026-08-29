use garive_ledger::{CommitResult, SessionId};
use garive_scheduler::{ScheduleErrorCode, ScheduleIntent};

use crate::{
    plan_schedule_cancelled, plan_schedule_created, ScheduleCancelReason, ScheduleLifecycleContext,
    SqliteLedger, SqliteLedgerError,
};

use super::{reconstruct_schedule_state, ScheduleAuthorityOperation, ScheduleAuthorityPort};

/// Creates one new immutable schedule revision after current authority validation.
pub fn create_schedule(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    expected_session_version: u64,
    command_id: &str,
    intent: &ScheduleIntent,
    recorded_at: &str,
    authority: &dyn ScheduleAuthorityPort,
) -> Result<CommitResult, ScheduleErrorCode> {
    authority.authorize(session_id, intent, ScheduleAuthorityOperation::Create)?;
    let fact = plan_schedule_created(
        &ScheduleLifecycleContext {
            recorded_at: recorded_at.into(),
        },
        command_id,
        intent,
    )
    .map_err(|_| ScheduleErrorCode::InvalidSchedule)?;
    ledger
        .commit(session_id.clone(), expected_session_version, vec![fact])
        .map_err(map_management)
}

/// Atomically supersedes the active revision and creates its replacement.
#[allow(clippy::too_many_arguments)]
pub fn update_schedule(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    expected_session_version: u64,
    command_id: &str,
    expected_revision_id: &str,
    replacement: &ScheduleIntent,
    recorded_at: &str,
    authority: &dyn ScheduleAuthorityPort,
) -> Result<CommitResult, ScheduleErrorCode> {
    let current = reconstruct_schedule_state(ledger, session_id, replacement.schedule_id())?;
    if !current.active
        || current.intent.revision_id() != expected_revision_id
        || replacement.revision_id() == expected_revision_id
    {
        return Err(ScheduleErrorCode::RevisionConflict);
    }
    authority.authorize(
        session_id,
        &current.intent,
        ScheduleAuthorityOperation::Update,
    )?;
    authority.authorize(session_id, replacement, ScheduleAuthorityOperation::Update)?;
    let context = ScheduleLifecycleContext {
        recorded_at: recorded_at.into(),
    };
    let cancelled = plan_schedule_cancelled(
        &context,
        command_id,
        replacement.schedule_id(),
        expected_revision_id,
        ScheduleCancelReason::Superseded,
    )
    .map_err(|_| ScheduleErrorCode::InvalidSchedule)?;
    let created = plan_schedule_created(&context, command_id, replacement)
        .map_err(|_| ScheduleErrorCode::InvalidSchedule)?;
    ledger
        .commit(
            session_id.clone(),
            expected_session_version,
            vec![cancelled, created],
        )
        .map_err(map_management)
}

/// Cancels one exact active revision after revalidating current ownership.
#[allow(clippy::too_many_arguments)]
pub fn cancel_schedule(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    expected_session_version: u64,
    command_id: &str,
    schedule_id: &str,
    expected_revision_id: &str,
    reason: ScheduleCancelReason,
    recorded_at: &str,
    authority: &dyn ScheduleAuthorityPort,
) -> Result<CommitResult, ScheduleErrorCode> {
    let current = reconstruct_schedule_state(ledger, session_id, schedule_id)?;
    if !current.active || current.intent.revision_id() != expected_revision_id {
        return Err(ScheduleErrorCode::RevisionConflict);
    }
    authority.authorize(
        session_id,
        &current.intent,
        ScheduleAuthorityOperation::Cancel,
    )?;
    let fact = plan_schedule_cancelled(
        &ScheduleLifecycleContext {
            recorded_at: recorded_at.into(),
        },
        command_id,
        schedule_id,
        expected_revision_id,
        reason,
    )
    .map_err(|_| ScheduleErrorCode::InvalidSchedule)?;
    ledger
        .commit(session_id.clone(), expected_session_version, vec![fact])
        .map_err(map_management)
}

fn map_management(error: SqliteLedgerError) -> ScheduleErrorCode {
    match error {
        SqliteLedgerError::Storage(_) => ScheduleErrorCode::DurabilityFailure,
        SqliteLedgerError::Domain(garive_ledger::LedgerError::ConcurrentModification)
        | SqliteLedgerError::Domain(garive_ledger::LedgerError::InvalidTransition) => {
            ScheduleErrorCode::RevisionConflict
        }
        _ => ScheduleErrorCode::CorruptScheduleState,
    }
}
