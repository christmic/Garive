use std::{error::Error, fmt};

use garive_ledger::{FactDraft, SessionId};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;

use super::storage;

/// Explicit request for one occurrence-scoped operational lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleLeaseRequest {
    /// Session owning the schedule.
    pub session_id: SessionId,
    /// Durable schedule identity.
    pub schedule_id: String,
    /// Exact immutable active revision.
    pub revision_id: String,
    /// Deterministic due occurrence identity.
    pub occurrence_id: String,
    /// Non-zero revision-local occurrence ordinal.
    pub ordinal: u64,
    /// Stable worker/process identity.
    pub owner_id: String,
    /// Unpredictable token used on every protected write.
    pub lease_id: String,
    /// Explicit current monotonic time in milliseconds.
    pub now_ms: u64,
    /// Non-zero bounded lease duration in milliseconds.
    pub duration_ms: u64,
}

/// Acquired fencing proof for exactly one schedule occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleLease {
    pub(crate) session_id: SessionId,
    pub(crate) schedule_id: String,
    pub(crate) revision_id: String,
    pub(crate) occurrence_id: String,
    pub(crate) ordinal: u64,
    pub(crate) owner_id: String,
    pub(crate) lease_id: String,
    /// Monotonic takeover epoch committed in `schedule.claimed`.
    pub epoch: u64,
    /// Explicit monotonic expiry instant.
    pub expires_at_ms: u64,
}

/// Stable schedule lease failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleLeaseError {
    /// An identity is empty, a bound is zero, or expiry arithmetic overflowed.
    InvalidRequest,
    /// Another unexpired worker owns this schedule.
    AlreadyHeld,
    /// The requested revision is not currently active in this Session.
    RevisionNotActive,
    /// The proof is stale, expired, or was taken over.
    LeaseLost,
    /// Protected facts do not name the leased occurrence.
    FactBindingMismatch,
    /// Durable or operational storage is corrupt/unavailable.
    Storage,
}

impl fmt::Display for ScheduleLeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "invalid schedule lease request",
            Self::AlreadyHeld => "schedule lease already held",
            Self::RevisionNotActive => "schedule revision is not active",
            Self::LeaseLost => "schedule lease ownership was lost",
            Self::FactBindingMismatch => "schedule fact does not match lease",
            Self::Storage => "schedule lease storage failure",
        })
    }
}

impl Error for ScheduleLeaseError {}

pub(super) fn acquire(
    transaction: &Transaction<'_>,
    request: &ScheduleLeaseRequest,
) -> Result<ScheduleLease, ScheduleLeaseError> {
    validate_request(request)?;
    require_active_revision(transaction, request)?;
    let expires_at_ms = request
        .now_ms
        .checked_add(request.duration_ms)
        .ok_or(ScheduleLeaseError::InvalidRequest)?;
    let existing = transaction
        .query_row(
            "SELECT revision_id,occurrence_id,ordinal,owner_id,lease_id,epoch,expires_at_ms \
             FROM schedule_leases WHERE session_id=?1 AND schedule_id=?2",
            params![request.session_id.as_str(), request.schedule_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ScheduleLeaseError::Storage)?;
    let epoch = match existing {
        None => 1,
        Some((revision, occurrence, ordinal, owner, lease, epoch, expires)) => {
            let ordinal = decode(&ordinal)?;
            let epoch = decode(&epoch)?;
            let expires = decode(&expires)?;
            let exact = revision == request.revision_id
                && occurrence == request.occurrence_id
                && ordinal == request.ordinal
                && owner == request.owner_id
                && lease == request.lease_id;
            if exact && expires > request.now_ms {
                epoch
            } else if expires > request.now_ms
                && revision == request.revision_id
                && !occurrence_handled(transaction, request, &revision, &occurrence, ordinal)?
            {
                return Err(ScheduleLeaseError::AlreadyHeld);
            } else {
                epoch.checked_add(1).ok_or(ScheduleLeaseError::Storage)?
            }
        }
    };
    transaction
        .execute(
            "INSERT INTO schedule_leases(session_id,schedule_id,revision_id,occurrence_id,ordinal,owner_id,lease_id,epoch,expires_at_ms) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(session_id,schedule_id) DO UPDATE SET \
             revision_id=excluded.revision_id,occurrence_id=excluded.occurrence_id, \
             ordinal=excluded.ordinal,owner_id=excluded.owner_id,lease_id=excluded.lease_id, \
             epoch=excluded.epoch,expires_at_ms=excluded.expires_at_ms",
            params![
                request.session_id.as_str(), request.schedule_id, request.revision_id,
                request.occurrence_id, storage::encode_u64(request.ordinal), request.owner_id,
                request.lease_id, storage::encode_u64(epoch), storage::encode_u64(expires_at_ms),
            ],
        )
        .map_err(|_| ScheduleLeaseError::Storage)?;
    Ok(ScheduleLease {
        session_id: request.session_id.clone(),
        schedule_id: request.schedule_id.clone(),
        revision_id: request.revision_id.clone(),
        occurrence_id: request.occurrence_id.clone(),
        ordinal: request.ordinal,
        owner_id: request.owner_id.clone(),
        lease_id: request.lease_id.clone(),
        epoch,
        expires_at_ms,
    })
}

pub(super) fn require_owned(
    transaction: &Transaction<'_>,
    lease: &ScheduleLease,
    now_ms: u64,
) -> Result<(), ScheduleLeaseError> {
    if now_ms >= lease.expires_at_ms {
        return Err(ScheduleLeaseError::LeaseLost);
    }
    let owned: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schedule_leases WHERE session_id=?1 AND schedule_id=?2 \
             AND revision_id=?3 AND occurrence_id=?4 AND ordinal=?5 AND owner_id=?6 \
             AND lease_id=?7 AND epoch=?8 AND expires_at_ms=?9)",
            params![
                lease.session_id.as_str(),
                lease.schedule_id,
                lease.revision_id,
                lease.occurrence_id,
                storage::encode_u64(lease.ordinal),
                lease.owner_id,
                lease.lease_id,
                storage::encode_u64(lease.epoch),
                storage::encode_u64(lease.expires_at_ms),
            ],
            |row| row.get(0),
        )
        .map_err(|_| ScheduleLeaseError::Storage)?;
    if owned {
        Ok(())
    } else {
        Err(ScheduleLeaseError::LeaseLost)
    }
}

pub(super) fn require_bound_facts(
    lease: &ScheduleLease,
    drafts: &[FactDraft],
) -> Result<(), ScheduleLeaseError> {
    if drafts.is_empty() {
        return Err(ScheduleLeaseError::FactBindingMismatch);
    }
    for draft in drafts {
        let value: Value = serde_json::from_str(draft.payload.as_json())
            .map_err(|_| ScheduleLeaseError::FactBindingMismatch)?;
        if !matches!(
            draft.kind.as_str(),
            "schedule.claimed" | "schedule.fired" | "schedule.failed"
        ) || value.get("schedule_id").and_then(Value::as_str) != Some(&lease.schedule_id)
            || value.get("revision_id").and_then(Value::as_str) != Some(&lease.revision_id)
            || value.get("occurrence_id").and_then(Value::as_str) != Some(&lease.occurrence_id)
            || value.get("ordinal").and_then(Value::as_u64) != Some(lease.ordinal)
        {
            return Err(ScheduleLeaseError::FactBindingMismatch);
        }
    }
    Ok(())
}

pub(super) fn release(
    transaction: &Transaction<'_>,
    lease: &ScheduleLease,
) -> Result<(), ScheduleLeaseError> {
    if !occurrence_handled_raw(transaction, lease)? {
        return Err(ScheduleLeaseError::FactBindingMismatch);
    }
    let deleted = transaction
        .execute(
            "DELETE FROM schedule_leases WHERE session_id=?1 AND schedule_id=?2 AND lease_id=?3 AND epoch=?4",
            params![
                lease.session_id.as_str(),
                lease.schedule_id,
                lease.lease_id,
                storage::encode_u64(lease.epoch)
            ],
        )
        .map_err(|_| ScheduleLeaseError::Storage)?;
    if deleted == 1 {
        Ok(())
    } else {
        Err(ScheduleLeaseError::LeaseLost)
    }
}

fn validate_request(request: &ScheduleLeaseRequest) -> Result<(), ScheduleLeaseError> {
    if request.schedule_id.is_empty()
        || request.revision_id.is_empty()
        || request.occurrence_id.is_empty()
        || request.ordinal == 0
        || request.owner_id.is_empty()
        || request.lease_id.is_empty()
        || request.duration_ms == 0
    {
        Err(ScheduleLeaseError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn require_active_revision(
    transaction: &Transaction<'_>,
    request: &ScheduleLeaseRequest,
) -> Result<(), ScheduleLeaseError> {
    let events = schedule_events(transaction, &request.session_id, &request.schedule_id)?;
    let mut active: Option<String> = None;
    for (kind, value) in events {
        match kind.as_str() {
            "schedule.created" => {
                active = value
                    .get("revision_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            "schedule.cancelled"
                if active.as_deref()
                    == value.get("expected_revision_id").and_then(Value::as_str) =>
            {
                active = None;
            }
            "schedule.failed"
                if active.as_deref() == value.get("revision_id").and_then(Value::as_str) =>
            {
                active = None;
            }
            _ => {}
        }
    }
    if active.as_deref() == Some(&request.revision_id) {
        Ok(())
    } else {
        Err(ScheduleLeaseError::RevisionNotActive)
    }
}

fn occurrence_handled(
    transaction: &Transaction<'_>,
    request: &ScheduleLeaseRequest,
    revision_id: &str,
    occurrence_id: &str,
    ordinal: u64,
) -> Result<bool, ScheduleLeaseError> {
    occurrence_handled_values(
        transaction,
        &request.session_id,
        &request.schedule_id,
        revision_id,
        occurrence_id,
        ordinal,
    )
}

fn occurrence_handled_raw(
    transaction: &Transaction<'_>,
    lease: &ScheduleLease,
) -> Result<bool, ScheduleLeaseError> {
    occurrence_handled_values(
        transaction,
        &lease.session_id,
        &lease.schedule_id,
        &lease.revision_id,
        &lease.occurrence_id,
        lease.ordinal,
    )
}

fn occurrence_handled_values(
    transaction: &Transaction<'_>,
    session_id: &SessionId,
    schedule_id: &str,
    revision_id: &str,
    occurrence_id: &str,
    ordinal: u64,
) -> Result<bool, ScheduleLeaseError> {
    Ok(schedule_events(transaction, session_id, schedule_id)?
        .into_iter()
        .any(|(kind, value)| {
            matches!(kind.as_str(), "schedule.fired" | "schedule.failed")
                && value.get("revision_id").and_then(Value::as_str) == Some(revision_id)
                && value.get("occurrence_id").and_then(Value::as_str) == Some(occurrence_id)
                && value.get("ordinal").and_then(Value::as_u64) == Some(ordinal)
        }))
}

fn schedule_events(
    transaction: &Transaction<'_>,
    session_id: &SessionId,
    schedule_id: &str,
) -> Result<Vec<(String, Value)>, ScheduleLeaseError> {
    let mut statement = transaction
        .prepare(
            "SELECT kind,payload_json FROM ledger_facts WHERE session_id=?1 \
             AND kind LIKE 'schedule.%' ORDER BY position",
        )
        .map_err(|_| ScheduleLeaseError::Storage)?;
    let rows = statement
        .query_map([session_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| ScheduleLeaseError::Storage)?;
    let mut events = Vec::new();
    for row in rows {
        let (kind, raw) = row.map_err(|_| ScheduleLeaseError::Storage)?;
        let value: Value = serde_json::from_str(&raw).map_err(|_| ScheduleLeaseError::Storage)?;
        if value.get("schedule_id").and_then(Value::as_str) == Some(schedule_id) {
            events.push((kind, value));
        }
    }
    Ok(events)
}

fn decode(value: &[u8]) -> Result<u64, ScheduleLeaseError> {
    let bytes: [u8; 8] = value.try_into().map_err(|_| ScheduleLeaseError::Storage)?;
    Ok(u64::from_be_bytes(bytes))
}
