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

const MIGRATION_4: &str = r#"
CREATE TABLE memory_namespaces (
    namespace_id TEXT PRIMARY KEY NOT NULL CHECK(length(namespace_id) > 0),
    repository_revision BLOB NOT NULL CHECK(length(repository_revision) = 8)
) STRICT;

CREATE TABLE memory_control_journal (
    namespace_id TEXT NOT NULL REFERENCES memory_namespaces(namespace_id),
    sequence BLOB NOT NULL CHECK(length(sequence) = 8),
    event_id TEXT NOT NULL UNIQUE CHECK(length(event_id) > 0),
    command_id TEXT NOT NULL CHECK(length(command_id) > 0),
    event_kind TEXT NOT NULL CHECK(event_kind IN ('import', 'export')),
    schema_version INTEGER NOT NULL CHECK(schema_version = 1),
    binding_digest TEXT NOT NULL CHECK(length(binding_digest) = 64),
    previous_repository_revision BLOB NOT NULL CHECK(length(previous_repository_revision) = 8),
    committed_repository_revision BLOB NOT NULL CHECK(length(committed_repository_revision) = 8),
    operations_json TEXT,
    operations_sha256 TEXT CHECK(operations_sha256 IS NULL OR length(operations_sha256) = 64),
    receipt_json TEXT NOT NULL,
    receipt_sha256 TEXT NOT NULL CHECK(length(receipt_sha256) = 64),
    event_json TEXT NOT NULL,
    event_sha256 TEXT NOT NULL CHECK(length(event_sha256) = 64),
    PRIMARY KEY(namespace_id, sequence),
    UNIQUE(namespace_id, command_id),
    CHECK((event_kind = 'import') = (operations_json IS NOT NULL)),
    CHECK((operations_json IS NULL) = (operations_sha256 IS NULL))
) STRICT;

CREATE TABLE memory_control_revisions (
    namespace_id TEXT NOT NULL REFERENCES memory_namespaces(namespace_id),
    record_id TEXT NOT NULL CHECK(length(record_id) > 0),
    revision_id TEXT NOT NULL CHECK(length(revision_id) > 0),
    document_markdown TEXT,
    document_digest TEXT NOT NULL CHECK(length(document_digest) = 64),
    created_sequence BLOB NOT NULL CHECK(length(created_sequence) = 8),
    erased_sequence BLOB CHECK(erased_sequence IS NULL OR length(erased_sequence) = 8),
    CHECK((document_markdown IS NULL) = (erased_sequence IS NOT NULL)),
    PRIMARY KEY(namespace_id, record_id, revision_id)
) STRICT;

CREATE TABLE memory_control_current (
    namespace_id TEXT NOT NULL REFERENCES memory_namespaces(namespace_id),
    record_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    lifecycle TEXT NOT NULL CHECK(lifecycle IN (
        'candidate', 'active', 'cold', 'archived', 'promoted', 'erased'
    )),
    document_markdown TEXT,
    document_digest TEXT CHECK(document_digest IS NULL OR length(document_digest) = 64),
    updated_sequence BLOB NOT NULL CHECK(length(updated_sequence) = 8),
    PRIMARY KEY(namespace_id, record_id),
    CHECK((lifecycle = 'erased') = (document_markdown IS NULL)),
    CHECK((document_markdown IS NULL) = (document_digest IS NULL)),
    FOREIGN KEY(namespace_id, record_id, revision_id)
        REFERENCES memory_control_revisions(namespace_id, record_id, revision_id)
) STRICT;

CREATE INDEX memory_journal_by_command
    ON memory_control_journal(namespace_id, command_id);
CREATE INDEX memory_current_by_lifecycle
    ON memory_control_current(namespace_id, lifecycle, record_id);
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
    if version > 4 {
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
        version = 3;
    }
    if version == 3 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_4)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
    }
    Ok(())
}
