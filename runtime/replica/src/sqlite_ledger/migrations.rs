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

const MIGRATION_2: &str = r#"
CREATE TABLE execution_leases (
    turn_id TEXT PRIMARY KEY NOT NULL,
    execution_id TEXT NOT NULL,
    owner_id TEXT NOT NULL CHECK(length(owner_id) > 0),
    lease_token TEXT NOT NULL UNIQUE CHECK(length(lease_token) > 0),
    generation BLOB NOT NULL CHECK(length(generation) = 8),
    expires_at_ms BLOB NOT NULL CHECK(length(expires_at_ms) = 8)
) STRICT;
"#;

const MIGRATION_3: &str = r#"
CREATE TABLE schedule_leases (
    session_id TEXT NOT NULL REFERENCES ledger_sessions(session_id),
    schedule_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    occurrence_id TEXT NOT NULL,
    ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
    owner_id TEXT NOT NULL CHECK(length(owner_id) > 0),
    lease_id TEXT NOT NULL CHECK(length(lease_id) > 0),
    epoch BLOB NOT NULL CHECK(length(epoch) = 8),
    expires_at_ms BLOB NOT NULL CHECK(length(expires_at_ms) = 8),
    PRIMARY KEY(session_id, schedule_id),
    UNIQUE(session_id, lease_id)
) STRICT;
"#;

pub(super) fn migrate(connection: &mut Connection) -> Result<(), SqliteLedgerError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
         version INTEGER PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL\
         ) STRICT;",
    )?;
    let mut version: u32 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if version > 3 {
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
        version = 1;
    }
    if version == 1 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_2)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
        version = 2;
    }
    if version == 2 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_3)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
    }
    Ok(())
}
