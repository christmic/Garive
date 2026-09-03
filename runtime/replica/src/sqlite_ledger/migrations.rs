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

const MIGRATION_5: &str = r#"
ALTER TABLE memory_namespaces ADD COLUMN source_mode TEXT NOT NULL DEFAULT 'isolated'
    CHECK(source_mode IN ('isolated', 'fact_backed'));

CREATE TABLE memory_control_sources (
    namespace_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    source_position BLOB NOT NULL CHECK(length(source_position) = 8),
    source_fact_id TEXT NOT NULL UNIQUE,
    source_payload_digest TEXT NOT NULL CHECK(length(source_payload_digest) = 64),
    classification_fact_id TEXT NOT NULL UNIQUE,
    classification_payload_digest TEXT NOT NULL CHECK(length(classification_payload_digest) = 64),
    repository_revision BLOB NOT NULL CHECK(length(repository_revision) = 8),
    PRIMARY KEY(namespace_id, record_id, revision_id),
    FOREIGN KEY(namespace_id, record_id, revision_id)
        REFERENCES memory_control_revisions(namespace_id, record_id, revision_id),
    FOREIGN KEY(source_fact_id) REFERENCES ledger_facts(fact_id),
    FOREIGN KEY(classification_fact_id) REFERENCES ledger_facts(fact_id)
) STRICT;
"#;

const MIGRATION_6: &str = r#"
CREATE TABLE memory_repository_transitions (
    namespace_id TEXT NOT NULL REFERENCES memory_namespaces(namespace_id),
    record_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    transition_kind TEXT NOT NULL CHECK(transition_kind IN ('tombstone', 'lifecycle')),
    fact_id TEXT NOT NULL UNIQUE REFERENCES ledger_facts(fact_id),
    payload_digest TEXT NOT NULL CHECK(length(payload_digest) = 64),
    repository_revision BLOB NOT NULL CHECK(length(repository_revision) = 8),
    PRIMARY KEY(namespace_id, repository_revision)
) STRICT;
CREATE UNIQUE INDEX memory_source_by_repository_revision
    ON memory_control_sources(namespace_id, repository_revision);
"#;

const MIGRATION_7: &str = r#"
DROP INDEX memory_source_by_repository_revision;
ALTER TABLE memory_control_sources ADD COLUMN operation_ordinal BLOB NOT NULL
    DEFAULT X'0000000000000000' CHECK(length(operation_ordinal) = 8);
CREATE UNIQUE INDEX memory_source_by_repository_operation
    ON memory_control_sources(namespace_id, repository_revision, operation_ordinal);

CREATE TABLE memory_repository_transitions_v7 (
    namespace_id TEXT NOT NULL REFERENCES memory_namespaces(namespace_id),
    record_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    transition_kind TEXT NOT NULL CHECK(transition_kind IN ('tombstone', 'lifecycle')),
    fact_id TEXT NOT NULL UNIQUE REFERENCES ledger_facts(fact_id),
    payload_digest TEXT NOT NULL CHECK(length(payload_digest) = 64),
    repository_revision BLOB NOT NULL CHECK(length(repository_revision) = 8),
    operation_ordinal BLOB NOT NULL DEFAULT X'0000000000000000'
        CHECK(length(operation_ordinal) = 8),
    PRIMARY KEY(namespace_id, repository_revision, operation_ordinal)
) STRICT;
INSERT INTO memory_repository_transitions_v7(
    namespace_id,record_id,revision_id,transition_kind,fact_id,payload_digest,
    repository_revision,operation_ordinal
)
SELECT namespace_id,record_id,revision_id,transition_kind,fact_id,payload_digest,
       repository_revision,X'0000000000000000'
FROM memory_repository_transitions;
DROP TABLE memory_repository_transitions;
ALTER TABLE memory_repository_transitions_v7 RENAME TO memory_repository_transitions;
"#;

const MIGRATION_8: &str = r#"
CREATE TABLE runtime_monotonic_clock (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    boot_revision TEXT NOT NULL CHECK(length(boot_revision) > 0),
    boot_origin_tick BLOB NOT NULL CHECK(length(boot_origin_tick) = 8),
    logical_origin_tick BLOB NOT NULL CHECK(length(logical_origin_tick) = 8),
    last_tick BLOB NOT NULL CHECK(length(last_tick) = 8),
    reserved_until_tick BLOB NOT NULL CHECK(length(reserved_until_tick) = 8)
) STRICT;
"#;

const MIGRATION_9: &str = r#"
CREATE TABLE runtime_management_config (
    config_id INTEGER PRIMARY KEY NOT NULL CHECK(config_id = 1),
    profile_id TEXT NOT NULL CHECK(length(profile_id) > 0),
    endpoint_override TEXT,
    model_target_id TEXT NOT NULL CHECK(length(model_target_id) > 0),
    model_id TEXT NOT NULL CHECK(length(model_id) > 0),
    deployment_id TEXT NOT NULL CHECK(length(deployment_id) > 0),
    definition_id TEXT NOT NULL CHECK(length(definition_id) > 0),
    api_key TEXT NOT NULL CHECK(length(api_key) > 0),
    runtime_id TEXT NOT NULL CHECK(length(runtime_id) > 0),
    configuration_revision INTEGER NOT NULL CHECK(configuration_revision > 0),
    configuration_digest TEXT NOT NULL CHECK(length(configuration_digest) = 64),
    committed_at TEXT NOT NULL CHECK(length(committed_at) > 0),
    CHECK(endpoint_override IS NULL OR length(endpoint_override) > 0)
) STRICT;
"#;

const MIGRATION_10: &str = r#"
CREATE TABLE registered_agents (
    agent_id TEXT PRIMARY KEY NOT NULL CHECK(length(agent_id) BETWEEN 1 AND 64),
    working_directory TEXT NOT NULL CHECK(length(working_directory) > 0),
    readonly_knowledge_directories_json TEXT NOT NULL,
    writable_knowledge_directory TEXT,
    status TEXT NOT NULL CHECK(status IN ('inactive', 'active', 'archived')),
    CHECK(writable_knowledge_directory IS NULL OR length(writable_knowledge_directory) > 0)
) STRICT;

CREATE TABLE agent_registry_commands (
    command_id TEXT PRIMARY KEY NOT NULL CHECK(length(command_id) BETWEEN 1 AND 128),
    agent_id TEXT NOT NULL REFERENCES registered_agents(agent_id),
    operation TEXT NOT NULL CHECK(operation IN ('create', 'update_knowledge', 'activate', 'archive')),
    request_digest TEXT NOT NULL CHECK(length(request_digest) = 64),
    response_json TEXT NOT NULL
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
    if version > 10 {
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
        version = 4;
    }
    if version == 4 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_5)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
        version = 5;
    }
    if version == 5 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_6)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
        version = 6;
    }
    if version == 6 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_7)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
        version = 7;
    }
    if version == 7 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_8)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (8, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
        version = 8;
    }
    if version == 8 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_9)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (9, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
        version = 9;
    }
    if version == 9 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_10)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at) \
             VALUES (10, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        transaction.commit()?;
    }
    Ok(())
}
