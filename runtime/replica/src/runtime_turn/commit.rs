use garive_ledger::{CommitResult, LedgerError, SessionId};

use crate::{SqliteLedger, SqliteLedgerError};

use super::{PlannedTurn, RuntimeCommandError};

/// Commits one planned Runtime command and hides storage-specific error classes.
pub fn commit_planned_turn(
    ledger: &mut SqliteLedger,
    session_id: SessionId,
    expected_session_version: u64,
    plan: &PlannedTurn,
) -> Result<CommitResult, RuntimeCommandError> {
    ledger
        .commit(session_id, expected_session_version, plan.facts.clone())
        .map_err(map_error)
}

fn map_error(error: SqliteLedgerError) -> RuntimeCommandError {
    match error {
        SqliteLedgerError::Domain(
            LedgerError::IdempotencyCollision | LedgerError::IncompleteReplay,
        ) => RuntimeCommandError::CommandConflict,
        SqliteLedgerError::Domain(LedgerError::ConcurrentModification) => {
            RuntimeCommandError::ConcurrentModification
        }
        SqliteLedgerError::Storage(_) => RuntimeCommandError::DurabilityFailure,
        SqliteLedgerError::CorruptLedger(_)
        | SqliteLedgerError::UnsupportedSchema(_)
        | SqliteLedgerError::InvalidStoredValue(_) => RuntimeCommandError::CorruptLedger,
        SqliteLedgerError::Domain(_) => RuntimeCommandError::InvariantViolation,
    }
}
