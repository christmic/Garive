use std::{error::Error, fmt, path::Path, time::Duration};

use garive_ledger::LedgerError;
use rusqlite::{Connection, OpenFlags};

mod migrations;

pub struct SqliteLedger {
    connection: Connection,
}

#[derive(Debug)]
pub enum SqliteLedgerError {
    Domain(LedgerError),
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

    #[doc(hidden)]
    pub fn connection_for_test(&self) -> &Connection {
        &self.connection
    }
}

impl fmt::Display for SqliteLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "ledger domain error: {}", error.code()),
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
