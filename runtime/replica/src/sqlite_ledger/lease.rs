use std::{error::Error, fmt};

use garive_ledger::{ExecutionId, TurnId};
use rusqlite::{params, OptionalExtension, Transaction};

use super::storage;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Explicit request to acquire one Execution's operational write lease.
pub struct ExecutionLeaseRequest {
    /// Turn protected by the lease.
    pub turn_id: TurnId,
    /// Currently active disposable Execution.
    pub execution_id: ExecutionId,
    /// Stable Runtime process/worker owner identity.
    pub owner_id: String,
    /// Unpredictable Runtime-created token used on every protected commit.
    pub lease_token: String,
    /// Explicit current monotonic time in milliseconds.
    pub now_ms: u64,
    /// Non-zero lease duration in milliseconds.
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Acquired lease proof required by all writes made while Core executes.
pub struct ExecutionLease {
    pub(crate) turn_id: TurnId,
    pub(crate) execution_id: ExecutionId,
    pub(crate) owner_id: String,
    pub(crate) lease_token: String,
    /// Monotonic lease generation for diagnostic and test evidence.
    pub generation: u64,
    /// Explicit expiry instant supplied by Runtime arithmetic.
    pub expires_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable lease acquisition, ownership, or storage failure.
pub enum ExecutionLeaseError {
    /// Request contains an empty identity, zero duration, or overflowing expiry.
    InvalidRequest,
    /// Another non-expired owner currently holds the Turn lease.
    AlreadyHeld,
    /// The prior expired Execution must be durably recovered before takeover.
    RecoveryRequired,
    /// The requested Execution is not the latest active Execution of the Turn.
    ExecutionNotActive,
    /// The supplied token no longer owns the Turn lease.
    LeaseLost,
    /// Verified ledger or operational lease storage is corrupt/unavailable.
    Storage,
}

impl fmt::Display for ExecutionLeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "invalid execution lease request",
            Self::AlreadyHeld => "execution lease already held",
            Self::RecoveryRequired => "expired execution requires durable recovery",
            Self::ExecutionNotActive => "execution is not the latest active execution",
            Self::LeaseLost => "execution lease ownership was lost",
            Self::Storage => "execution lease storage failure",
        })
    }
}

impl Error for ExecutionLeaseError {}

pub(super) fn acquire(
    transaction: &Transaction<'_>,
    request: &ExecutionLeaseRequest,
) -> Result<ExecutionLease, ExecutionLeaseError> {
    if request.owner_id.is_empty() || request.lease_token.is_empty() || request.duration_ms == 0 {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let expires_at_ms = request
        .now_ms
        .checked_add(request.duration_ms)
        .ok_or(ExecutionLeaseError::InvalidRequest)?;
    require_latest_active(transaction, &request.turn_id, &request.execution_id)?;
    let existing = transaction
        .query_row(
            "SELECT execution_id, owner_id, lease_token, generation, expires_at_ms \
             FROM execution_leases WHERE turn_id = ?1",
            [request.turn_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ExecutionLeaseError::Storage)?;
    let generation = match existing {
        None => 1,
        Some((execution, owner, token, generation, expires)) => {
            let generation = decode(&generation)?;
            let expires = decode(&expires)?;
            if execution == request.execution_id.as_str()
                && owner == request.owner_id
                && token == request.lease_token
            {
                if expires <= request.now_ms {
                    return Err(ExecutionLeaseError::RecoveryRequired);
                }
                generation
            } else if execution != request.execution_id.as_str() {
                require_terminal(transaction, &request.turn_id, &execution)?;
                generation
                    .checked_add(1)
                    .ok_or(ExecutionLeaseError::Storage)?
            } else {
                if expires > request.now_ms {
                    return Err(ExecutionLeaseError::AlreadyHeld);
                }
                return Err(ExecutionLeaseError::RecoveryRequired);
            }
        }
    };
    transaction
        .execute(
            "INSERT INTO execution_leases(\
             turn_id, execution_id, owner_id, lease_token, generation, expires_at_ms\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(turn_id) DO UPDATE SET execution_id=excluded.execution_id, \
             owner_id=excluded.owner_id, lease_token=excluded.lease_token, \
             generation=excluded.generation, expires_at_ms=excluded.expires_at_ms",
            params![
                request.turn_id.as_str(),
                request.execution_id.as_str(),
                request.owner_id,
                request.lease_token,
                storage::encode_u64(generation),
                storage::encode_u64(expires_at_ms),
            ],
        )
        .map_err(|_| ExecutionLeaseError::Storage)?;
    Ok(ExecutionLease {
        turn_id: request.turn_id.clone(),
        execution_id: request.execution_id.clone(),
        owner_id: request.owner_id.clone(),
        lease_token: request.lease_token.clone(),
        generation,
        expires_at_ms,
    })
}

pub(super) fn require_owned(
    transaction: &Transaction<'_>,
    lease: &ExecutionLease,
) -> Result<(), ExecutionLeaseError> {
    let owned: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM execution_leases WHERE turn_id=?1 \
             AND execution_id=?2 AND owner_id=?3 AND lease_token=?4)",
            params![
                lease.turn_id.as_str(),
                lease.execution_id.as_str(),
                lease.owner_id,
                lease.lease_token,
            ],
            |row| row.get(0),
        )
        .map_err(|_| ExecutionLeaseError::Storage)?;
    if owned {
        Ok(())
    } else {
        Err(ExecutionLeaseError::LeaseLost)
    }
}

pub(super) fn release(
    transaction: &Transaction<'_>,
    lease: &ExecutionLease,
) -> Result<(), ExecutionLeaseError> {
    require_owned(transaction, lease)?;
    require_terminal(transaction, &lease.turn_id, lease.execution_id.as_str())?;
    transaction
        .execute(
            "DELETE FROM execution_leases WHERE turn_id=?1 AND lease_token=?2",
            params![lease.turn_id.as_str(), lease.lease_token],
        )
        .map_err(|_| ExecutionLeaseError::Storage)?;
    Ok(())
}

fn require_latest_active(
    transaction: &Transaction<'_>,
    turn_id: &TurnId,
    execution_id: &ExecutionId,
) -> Result<(), ExecutionLeaseError> {
    let snapshot = storage::load_state_in_transaction(transaction)
        .map_err(|_| ExecutionLeaseError::Storage)?
        .load_turn(turn_id)
        .map_err(|_| ExecutionLeaseError::ExecutionNotActive)?;
    let latest = snapshot
        .facts
        .iter()
        .rfind(|fact| fact.kind.as_str() == "execution.started")
        .and_then(|fact| fact.execution_id.as_ref());
    let terminal = snapshot.facts.iter().any(|fact| {
        fact.execution_id.as_ref() == Some(execution_id)
            && matches!(
                fact.kind.as_str(),
                "execution.completed"
                    | "execution.suspended"
                    | "execution.stopped"
                    | "execution.failed"
                    | "execution.abandoned"
            )
    });
    if latest == Some(execution_id) && !terminal {
        Ok(())
    } else {
        Err(ExecutionLeaseError::ExecutionNotActive)
    }
}

fn require_terminal(
    transaction: &Transaction<'_>,
    turn_id: &TurnId,
    execution: &str,
) -> Result<(), ExecutionLeaseError> {
    let execution = ExecutionId::try_from(execution).map_err(|_| ExecutionLeaseError::Storage)?;
    let snapshot = storage::load_state_in_transaction(transaction)
        .map_err(|_| ExecutionLeaseError::Storage)?
        .load_turn(turn_id)
        .map_err(|_| ExecutionLeaseError::Storage)?;
    if snapshot.facts.iter().any(|fact| {
        fact.execution_id.as_ref() == Some(&execution)
            && matches!(
                fact.kind.as_str(),
                "execution.completed"
                    | "execution.suspended"
                    | "execution.stopped"
                    | "execution.failed"
                    | "execution.abandoned"
            )
    }) {
        Ok(())
    } else {
        Err(ExecutionLeaseError::RecoveryRequired)
    }
}

fn decode(value: &[u8]) -> Result<u64, ExecutionLeaseError> {
    let bytes: [u8; 8] = value.try_into().map_err(|_| ExecutionLeaseError::Storage)?;
    Ok(u64::from_be_bytes(bytes))
}
