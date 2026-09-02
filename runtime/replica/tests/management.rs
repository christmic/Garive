use garive_runtime::{ManagementCommitBody, ManagementConfigError, SqliteLedger};
use rusqlite::Connection;
use tempfile::tempdir;

fn sample_body() -> ManagementCommitBody {
    ManagementCommitBody {
        schema_version: 1,
        profile_id: "openai.responses.v1".to_string(),
        endpoint_override: Some("https://api.openai.com/v1".to_string()),
        model_target_id: "gpt-5.6".to_string(),
        model_id: "gpt-5.6".to_string(),
        deployment_id: "tok9-flash".to_string(),
        definition_id: "desktop.agent.v3".to_string(),
        api_key: "sk-test-1234567890".to_string(),
        runtime_id: "runtime-7e22bcbe-bfa4-4c8f-a0c3-94e07be8f363".to_string(),
    }
}

fn open_store() -> (tempfile::TempDir, SqliteLedger, Connection) {
    let directory = tempdir().unwrap();
    let path = directory.path().join("ledger.sqlite3");
    let ledger = SqliteLedger::open(&path).unwrap();
    // Open a parallel connection only to assert on the persisted singleton.
    let raw = Connection::open(&path).unwrap();
    (directory, ledger, raw)
}

#[test]
fn empty_ledger_reads_as_none() {
    let (_dir, mut ledger, _raw) = open_store();
    let store = ledger.management_config_store();
    assert!(matches!(store.read(), Ok(None)));
}

#[test]
fn commit_then_read_roundtrips() {
    let (_dir, mut ledger, raw) = open_store();
    let body = sample_body();
    let mut store = ledger.management_config_store();
    let receipt = store.commit(&body, "2026-09-02T00:00:00Z").unwrap();
    assert_eq!(receipt.schema_version, 1);
    assert_eq!(receipt.configuration_revision, 1);
    assert!(receipt.restart_required);
    assert_eq!(receipt.configuration_digest.as_str().len(), 64);
    assert_eq!(receipt.receipt_digest.as_str().len(), 64);

    let state = ledger.management_config_store().read().unwrap().unwrap();
    assert_eq!(state.profile_id, body.profile_id);
    assert_eq!(state.definition_id, body.definition_id);
    assert_eq!(state.runtime_id, body.runtime_id);
    assert_eq!(state.configuration_revision, 1);
    assert_eq!(state.configuration_digest, receipt.configuration_digest);
    assert_eq!(state.committed_at, "2026-09-02T00:00:00Z");

    // API key must NOT be returned by read paths (reductable).
    let api_key: String = raw
        .query_row(
            "SELECT api_key FROM runtime_management_config WHERE config_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(api_key, body.api_key);
}

#[test]
fn commit_bumps_revision_monotonically() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    let r1 = store.commit(&body, "2026-09-02T00:00:00Z").unwrap();
    assert_eq!(r1.configuration_revision, 1);

    body.api_key = "sk-test-changed".to_string();
    let r2 = store.commit(&body, "2026-09-02T00:01:00Z").unwrap();
    assert_eq!(r2.configuration_revision, 2);

    body.endpoint_override = None;
    let r3 = store.commit(&body, "2026-09-02T00:02:00Z").unwrap();
    assert_eq!(r3.configuration_revision, 3);

    // Configuration digests must differ across any field change.
    assert_ne!(r1.configuration_digest, r2.configuration_digest);
    assert_ne!(r2.configuration_digest, r3.configuration_digest);
}

#[test]
fn clear_removes_singleton() {
    let (_dir, mut ledger, raw) = open_store();
    let mut store = ledger.management_config_store();
    store
        .commit(&sample_body(), "2026-09-02T00:00:00Z")
        .unwrap();
    let count_before: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM runtime_management_config",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count_before, 1);
    store.clear().unwrap();
    let count_after: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM runtime_management_config",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count_after, 0);
    assert!(matches!(ledger.management_config_store().read(), Ok(None)));
}

#[test]
fn schema_version_mismatch_is_rejected() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    body.schema_version = 999;
    assert!(matches!(
        store.commit(&body, "2026-09-02T00:00:00Z"),
        Err(ManagementConfigError::SchemaVersionUnsupported)
    ));
}

#[test]
fn empty_api_key_is_rejected() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    body.api_key = "   ".to_string();
    assert!(matches!(
        store.commit(&body, "2026-09-02T00:00:00Z"),
        Err(ManagementConfigError::ApiKeyInvalid)
    ));
}

#[test]
fn api_key_at_byte_cap_is_accepted() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    body.api_key = "a".repeat(512);
    assert!(store.commit(&body, "2026-09-02T00:00:00Z").is_ok());
}

#[test]
fn api_key_over_byte_cap_is_rejected() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    body.api_key = "a".repeat(513);
    assert!(matches!(
        store.commit(&body, "2026-09-02T00:00:00Z"),
        Err(ManagementConfigError::ApiKeyInvalid)
    ));
}

#[test]
fn whitespace_endpoint_override_is_rejected() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    body.endpoint_override = Some("https://example.com/ bad".to_string());
    assert!(matches!(
        store.commit(&body, "2026-09-02T00:00:00Z"),
        Err(ManagementConfigError::EndpointInvalid)
    ));
}

#[test]
fn identifier_with_forbidden_chars_is_rejected() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let mut body = sample_body();
    body.profile_id = "openai/responses".to_string();
    assert!(matches!(
        store.commit(&body, "2026-09-02T00:00:00Z"),
        Err(ManagementConfigError::IdentifierInvalid)
    ));
}

#[test]
fn singleton_constraint_rejects_duplicate_config_id() {
    let (_dir, mut ledger, raw) = open_store();
    ledger
        .management_config_store()
        .commit(&sample_body(), "2026-09-02T00:00:00Z")
        .unwrap();
    let result = raw.execute(
        "INSERT INTO runtime_management_config(\
            config_id, profile_id, model_target_id, model_id, deployment_id, \
            definition_id, api_key, runtime_id, configuration_revision, \
            configuration_digest, committed_at\
         ) VALUES (2, ?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, '2026-09-02T00:00:00Z')",
        rusqlite::params_from_iter([
            "openai.responses.v1",
            "gpt-5.6",
            "gpt-5.6",
            "tok9-flash",
            "desktop.agent.v3",
            "api-key",
            "runtime-id",
            &"0".repeat(64),
        ]),
    );
    assert!(
        result.is_err(),
        "config_id must be constrained to exactly 1 row"
    );
}

#[test]
fn digest_changes_when_api_key_changes() {
    let (_dir, mut ledger, _raw) = open_store();
    let mut store = ledger.management_config_store();
    let body_a = sample_body();
    let r_a = store.commit(&body_a, "2026-09-02T00:00:00Z").unwrap();

    let mut body_b = sample_body();
    body_b.api_key = "sk-other-key".to_string();
    let r_b = store.commit(&body_b, "2026-09-02T00:01:00Z").unwrap();

    assert_ne!(r_a.configuration_digest, r_b.configuration_digest);
}

#[test]
fn wire_codes_are_stable() {
    assert_eq!(
        ManagementConfigError::ProfileUnknown.wire_code(),
        "management_profile_unknown"
    );
    assert_eq!(
        ManagementConfigError::DefinitionUnknown.wire_code(),
        "management_definition_unknown"
    );
    assert_eq!(
        ManagementConfigError::EndpointInvalid.wire_code(),
        "management_endpoint_invalid"
    );
    assert_eq!(
        ManagementConfigError::ApiKeyInvalid.wire_code(),
        "management_api_key_invalid"
    );
    assert_eq!(
        ManagementConfigError::StorageFailed.wire_code(),
        "management_storage_failed"
    );
    assert_eq!(
        ManagementConfigError::NotConfigured.wire_code(),
        "management_not_configured"
    );
}
