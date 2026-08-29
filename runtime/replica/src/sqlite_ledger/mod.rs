use std::{error::Error, fmt, path::Path, time::Duration};

use garive_ledger::{
    CommitDisposition, CommitResult, DurableFact, FactDraft, FactKind, LedgerError, ModelRequestId,
    SessionId, ToolInvocationId, TurnId, TurnSnapshot,
};
use rusqlite::{params, Connection, OpenFlags, Transaction, TransactionBehavior};

mod lease;
mod migrations;
mod storage;

pub use lease::{ExecutionLease, ExecutionLeaseError, ExecutionLeaseRequest};

/// SQLite-backed durable Ledger adapter with restart-safe append semantics.
pub struct SqliteLedger {
    connection: Connection,
}

/// Current durable coordinates of one Session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionWatermark {
    /// Optimistic-concurrency version advanced once per committed batch.
    pub session_version: u64,
    /// Highest contiguous fact position in the Session.
    pub max_position: u64,
}

#[derive(Debug)]
/// Domain, integrity, migration, or storage failure from [`SqliteLedger`].
pub enum SqliteLedgerError {
    /// A submitted operation violated the portable Ledger contract.
    Domain(LedgerError),
    /// Persisted rows could not reconstruct a valid portable Ledger state.
    CorruptLedger(LedgerError),
    /// SQLite returned an operational/storage error.
    Storage(rusqlite::Error),
    /// Database schema is newer than this adapter understands.
    UnsupportedSchema(u32),
    /// A persisted value cannot represent its declared domain field.
    InvalidStoredValue(&'static str),
    /// An execution-side commit no longer owns its operational lease.
    Lease(ExecutionLeaseError),
}

impl SqliteLedger {
    /// Opens or creates a database and enforces WAL, foreign keys, FULL sync, and migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteLedgerError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(path, flags)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let journal_mode: String =
            connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(SqliteLedgerError::InvalidStoredValue("journal_mode"));
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        migrations::migrate(&mut connection)?;
        Ok(Self { connection })
    }

    /// Atomically validates and appends one portable fact batch.
    ///
    /// Uses an immediate transaction so version comparison, contiguous position
    /// allocation, fact insertion, and projection advancement commit together.
    pub fn commit(
        &mut self,
        session_id: SessionId,
        expected_session_version: u64,
        drafts: Vec<FactDraft>,
    ) -> Result<CommitResult, SqliteLedgerError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result =
            commit_transaction(&transaction, session_id, expected_session_version, drafts)?;
        transaction.commit()?;
        Ok(result)
    }

    /// Acquires or renews one latest-active Execution lease transactionally.
    pub fn acquire_execution_lease(
        &mut self,
        request: &ExecutionLeaseRequest,
    ) -> Result<ExecutionLease, ExecutionLeaseError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ExecutionLeaseError::Storage)?;
        let lease = lease::acquire(&transaction, request)?;
        transaction
            .commit()
            .map_err(|_| ExecutionLeaseError::Storage)?;
        Ok(lease)
    }

    /// Appends facts only while the exact Execution lease still owns the Turn.
    pub fn commit_leased(
        &mut self,
        lease: &ExecutionLease,
        session_id: SessionId,
        expected_session_version: u64,
        drafts: Vec<FactDraft>,
    ) -> Result<CommitResult, SqliteLedgerError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        lease::require_owned(&transaction, lease).map_err(SqliteLedgerError::Lease)?;
        let result =
            commit_transaction(&transaction, session_id, expected_session_version, drafts)?;
        transaction.commit()?;
        Ok(result)
    }

    /// Releases a terminal Execution's lease using its exact ownership token.
    pub fn release_execution_lease(
        &mut self,
        lease: &ExecutionLease,
    ) -> Result<(), ExecutionLeaseError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ExecutionLeaseError::Storage)?;
        lease::release(&transaction, lease)?;
        transaction
            .commit()
            .map_err(|_| ExecutionLeaseError::Storage)
    }

    /// Reads a verified fixed-prefix fact range in ascending durable position.
    pub fn read_facts(
        &self,
        session_id: &SessionId,
        after_position: u64,
        through_position: u64,
        kinds: Option<&std::collections::BTreeSet<FactKind>>,
    ) -> Result<Vec<DurableFact>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.read_facts(
            session_id,
            after_position,
            through_position,
            kinds,
        )?)
    }

    /// Lists model requests still `Started` after reconstructing durable state.
    pub fn list_uncertain_model_requests(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ModelRequestId>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.list_uncertain_model_requests(session_id)?)
    }

    /// Lists effects still `Started` without a receipt or terminal fact.
    pub fn list_uncertain_tool_invocations(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ToolInvocationId>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.list_uncertain_tool_invocations(session_id)?)
    }

    /// Returns all verified lifecycle facts for one tool invocation.
    pub fn find_tool_invocation(
        &self,
        invocation_id: &ToolInvocationId,
    ) -> Result<Vec<DurableFact>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.find_tool_invocation(invocation_id))
    }

    /// Loads one verified Turn fact prefix and its Session watermark.
    pub fn load_turn(&self, turn_id: &TurnId) -> Result<TurnSnapshot, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.load_turn(turn_id)?)
    }

    /// Lists Open or Suspended Turn identities as recovery discovery hints.
    pub fn list_recoverable_turns(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<TurnId>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.list_recoverable_turns(session_id)?)
    }

    /// Returns the durable optimistic-concurrency version of a Session.
    pub fn session_version(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<u64>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.session_version(session_id))
    }

    /// Returns both current Session version and highest durable fact position.
    pub fn session_watermark(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SessionWatermark>, SqliteLedgerError> {
        let state = storage::load_state(&self.connection)?;
        let Some(session_version) = state.session_version(session_id) else {
            return Ok(None);
        };
        let max_position = u64::try_from(state.fact_count(session_id))
            .map_err(|_| SqliteLedgerError::InvalidStoredValue("session position"))?;
        Ok(Some(SessionWatermark {
            session_version,
            max_position,
        }))
    }

    #[doc(hidden)]
    pub fn connection_for_test(&self) -> &Connection {
        &self.connection
    }
}

fn commit_transaction(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    expected_session_version: u64,
    drafts: Vec<FactDraft>,
) -> Result<CommitResult, SqliteLedgerError> {
    let mut state = storage::load_state(transaction)?;
    let result = state.commit(session_id.clone(), expected_session_version, drafts.clone())?;
    if result.disposition == CommitDisposition::Replayed {
        return Ok(result);
    }
    transaction.execute(
        "INSERT OR IGNORE INTO ledger_sessions(session_id, version, max_position) \
         VALUES (?1, ?2, ?2)",
        params![session_id.as_str(), storage::encode_u64(0)],
    )?;
    let max_position = *result
        .positions
        .last()
        .ok_or(SqliteLedgerError::InvalidStoredValue("commit positions"))?;
    let updated = transaction.execute(
        "UPDATE ledger_sessions SET version = ?1, max_position = ?2 \
         WHERE session_id = ?3 AND version = ?4",
        params![
            storage::encode_u64(result.session_version),
            storage::encode_u64(max_position),
            session_id.as_str(),
            storage::encode_u64(expected_session_version),
        ],
    )?;
    if updated != 1 {
        return Err(SqliteLedgerError::Domain(
            LedgerError::ConcurrentModification,
        ));
    }
    for (draft, position) in drafts.iter().zip(&result.positions) {
        transaction.execute(
            "INSERT INTO ledger_facts(\
             fact_id, session_id, position, commit_version, turn_id, execution_id, \
             model_request_id, tool_invocation_id, kind, schema_version, payload_json, \
             payload_sha256, recorded_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                draft.fact_id.as_str(),
                session_id.as_str(),
                storage::encode_u64(*position),
                storage::encode_u64(result.session_version),
                draft.turn_id.as_ref().map(|value| value.as_str()),
                draft.execution_id.as_ref().map(|value| value.as_str()),
                draft.model_request_id.as_ref().map(|value| value.as_str()),
                draft
                    .tool_invocation_id
                    .as_ref()
                    .map(|value| value.as_str()),
                draft.kind.as_str(),
                i64::from(draft.schema_version),
                draft.payload.as_json(),
                draft.payload.sha256(),
                draft.recorded_at,
            ],
        )?;
    }
    Ok(result)
}

impl fmt::Display for SqliteLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "ledger domain error: {}", error.code()),
            Self::CorruptLedger(error) => {
                write!(formatter, "corrupt ledger: {}", error.code())
            }
            Self::Storage(error) => write!(formatter, "SQLite error: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(
                    formatter,
                    "unsupported SQLite ledger schema version {version}"
                )
            }
            Self::InvalidStoredValue(field) => write!(formatter, "invalid stored {field}"),
            Self::Lease(error) => write!(formatter, "execution lease error: {error}"),
        }
    }
}

impl Error for SqliteLedgerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Lease(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for SqliteLedgerError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage(value)
    }
}

impl From<LedgerError> for SqliteLedgerError {
    fn from(value: LedgerError) -> Self {
        Self::Domain(value)
    }
}
