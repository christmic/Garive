use rusqlite::Connection;

use super::SqliteLedgerError;

const MIGRATION_1: &str = r#"
CREATE TABLE ledger_sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    version BLOB NOT NULL CHECK(length(version) = 8),
    max_position BLOB NOT NULL CHECK(length(max_position) = 8)
) STRICT;

CREATE TABLE ledger_facts (
    fact_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES ledger_sessions(session_id),
    position BLOB NOT NULL CHECK(length(position) = 8),
    commit_version BLOB NOT NULL CHECK(length(commit_version) = 8),
    turn_id TEXT,
    execution_id TEXT,
    model_request_id TEXT,
    tool_invocation_id TEXT,
    kind TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK(schema_version > 0),
    payload_json TEXT NOT NULL,
    payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256) = 64),
    recorded_at TEXT NOT NULL,
    UNIQUE(session_id, position)
) STRICT;

CREATE UNIQUE INDEX one_model_prepared
    ON ledger_facts(model_request_id)
    WHERE kind = 'model.prepared';
CREATE UNIQUE INDEX one_effect_prepared
    ON ledger_facts(tool_invocation_id)
    WHERE kind = 'effect.prepared';
CREATE INDEX facts_by_session_version_position
    ON ledger_facts(session_id, commit_version, position);
CREATE INDEX facts_by_model_request
    ON ledger_facts(model_request_id, position)
    WHERE model_request_id IS NOT NULL;
CREATE INDEX facts_by_tool_invocation
    ON ledger_facts(tool_invocation_id, position)
    WHERE tool_invocation_id IS NOT NULL;
"#;

pub(super) fn migrate(connection: &mut Connection) -> Result<(), SqliteLedgerError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
         version INTEGER PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL\
         ) STRICT;",
    )?;
    let version: u32 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if version > 1 {
        return Err(SqliteLedgerError::UnsupportedSchema(version));
    }
    if version == 0 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_1)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
    }
    Ok(())
}
