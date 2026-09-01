use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{storage, SqliteLedger, SqliteLedgerError};

/// Stable revision written into every lease fact using this persistent clock.
pub const PERSISTENT_MONOTONIC_CLOCK_REVISION: &str = "garive-persistent-monotonic-v1";

/// One restart-safe logical monotonic lease reading.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentMonotonicReading {
    /// Stable cross-boot clock revision.
    pub clock_revision: String,
    /// Current logical tick after reserving the requested lease interval.
    pub now_ms: u64,
}

impl SqliteLedger {
    /// Reserves one lease interval on a persistent boot-aware monotonic clock.
    pub fn reserve_monotonic_lease(
        &mut self,
        boot_revision: &str,
        boot_tick_ms: u64,
        lease_duration_ms: u64,
    ) -> Result<PersistentMonotonicReading, SqliteLedgerError> {
        if boot_revision.is_empty() || lease_duration_ms == 0 {
            return Err(SqliteLedgerError::InvalidStoredValue(
                "monotonic_clock_input",
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT boot_revision, boot_origin_tick, logical_origin_tick, \
                 last_tick, reserved_until_tick FROM runtime_monotonic_clock \
                 WHERE singleton=1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                    ))
                },
            )
            .optional()?;
        let (boot_origin, logical_origin, now_ms, prior_reserved) =
            match existing {
                None => (boot_tick_ms, 1, 1, 0),
                Some((boot, boot_origin, logical_origin, last, reserved)) => {
                    let boot_origin = decode(&boot_origin, "monotonic_boot_origin")?;
                    let logical_origin = decode(&logical_origin, "monotonic_logical_origin")?;
                    let last = decode(&last, "monotonic_last_tick")?;
                    let reserved = decode(&reserved, "monotonic_reserved_until")?;
                    if boot == boot_revision {
                        let elapsed = boot_tick_ms
                            .checked_sub(boot_origin)
                            .ok_or(SqliteLedgerError::InvalidStoredValue("monotonic_boot_tick"))?;
                        let candidate = logical_origin.checked_add(elapsed).ok_or(
                            SqliteLedgerError::InvalidStoredValue("monotonic_logical_tick"),
                        )?;
                        (boot_origin, logical_origin, candidate.max(last), reserved)
                    } else {
                        let successor = reserved.checked_add(1).ok_or(
                            SqliteLedgerError::InvalidStoredValue("monotonic_boot_fence"),
                        )?;
                        (boot_tick_ms, successor, successor, reserved)
                    }
                }
            };
        let reserved_until = now_ms
            .checked_add(lease_duration_ms)
            .map(|value| value.max(prior_reserved))
            .ok_or(SqliteLedgerError::InvalidStoredValue(
                "monotonic_reservation",
            ))?;
        transaction.execute(
            "INSERT INTO runtime_monotonic_clock( \
             singleton, boot_revision, boot_origin_tick, logical_origin_tick, \
             last_tick, reserved_until_tick) VALUES(1, ?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(singleton) DO UPDATE SET \
             boot_revision=excluded.boot_revision, \
             boot_origin_tick=excluded.boot_origin_tick, \
             logical_origin_tick=excluded.logical_origin_tick, \
             last_tick=excluded.last_tick, \
             reserved_until_tick=excluded.reserved_until_tick",
            params![
                boot_revision,
                storage::encode_u64(boot_origin),
                storage::encode_u64(logical_origin),
                storage::encode_u64(now_ms),
                storage::encode_u64(reserved_until),
            ],
        )?;
        transaction.commit()?;
        Ok(PersistentMonotonicReading {
            clock_revision: PERSISTENT_MONOTONIC_CLOCK_REVISION.into(),
            now_ms,
        })
    }
}

fn decode(value: &[u8], field: &'static str) -> Result<u64, SqliteLedgerError> {
    value
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| SqliteLedgerError::InvalidStoredValue(field))
}
