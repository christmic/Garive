use std::{error::Error, fmt, path::Path, time::Duration};

use garive_ledger::{
    CommitDisposition, CommitResult, DurableFact, FactDraft, FactKind, LedgerError, ModelRequestId,
    SessionId, ToolInvocationId,
};
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};

mod migrations;
mod storage;

pub struct SqliteLedger {
    connection: Connection,
}

#[derive(Debug)]
pub enum SqliteLedgerError {
    Domain(LedgerError),
    CorruptLedger(LedgerError),
    Storage(rusqlite::Error),
    UnsupportedSchema(u32),
    InvalidStoredValue(&'static str),
}

impl SqliteLedger {
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

    pub fn commit(
        &mut self,
        session_id: SessionId,
        expected_session_version: u64,
        drafts: Vec<FactDraft>,
    ) -> Result<CommitResult, SqliteLedgerError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut state = storage::load_state(&transaction)?;
        let result = state.commit(session_id.clone(), expected_session_version, drafts.clone())?;
        if result.disposition == CommitDisposition::Replayed {
            transaction.commit()?;
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
        transaction.commit()?;
        Ok(result)
    }

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

    pub fn list_uncertain_model_requests(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ModelRequestId>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.list_uncertain_model_requests(session_id)?)
    }

    pub fn find_tool_invocation(
        &self,
        invocation_id: &ToolInvocationId,
    ) -> Result<Vec<DurableFact>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.find_tool_invocation(invocation_id))
    }

    pub fn session_version(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<u64>, SqliteLedgerError> {
        Ok(storage::load_state(&self.connection)?.session_version(session_id))
    }

    #[doc(hidden)]
    pub fn connection_for_test(&self) -> &Connection {
        &self.connection
    }
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
        }
    }
}

impl Error for SqliteLedgerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
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
