use std::{fs, path::PathBuf};

use garive_ledger::{
    CanonicalPayload, CommitDisposition, ExecutionId, FactDraft, FactId, FactKind, ModelRequestId,
    SessionId, ToolInvocationId, TurnId,
};
use garive_runtime::{SqliteLedger, SqliteLedgerError};
use serde_json::Value;
use tempfile::tempdir;

fn document(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn runtime_payload(kind: &str) -> Value {
    document("runtime-facts-v1.json")["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["kind"].as_str() == Some(kind))
        .map(|case| case["payload"].clone())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn draft(value: &Value) -> FactDraft {
    let mut payload = value
        .get("payload")
        .cloned()
        .unwrap_or_else(|| runtime_payload(value["kind"].as_str().unwrap()));
    if let Some(overrides) = value.get("payload_overrides").and_then(Value::as_object) {
        payload.as_object_mut().unwrap().extend(overrides.clone());
    }
    FactDraft {
        fact_id: FactId::try_from(value["id"].as_str().unwrap()).unwrap(),
        turn_id: value["turn"]
            .as_str()
            .map(|id| TurnId::try_from(id).unwrap()),
        execution_id: value["execution"]
            .as_str()
            .map(|id| ExecutionId::try_from(id).unwrap()),
        model_request_id: value["request"]
            .as_str()
            .map(|id| ModelRequestId::try_from(id).unwrap()),
        tool_invocation_id: value["tool"]
            .as_str()
            .map(|id| ToolInvocationId::try_from(id).unwrap()),
        kind: FactKind::new(value["kind"].as_str().unwrap()).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn error_code(error: SqliteLedgerError) -> &'static str {
    match error {
        SqliteLedgerError::Domain(error) | SqliteLedgerError::CorruptLedger(error) => error.code(),
        _ => "storage",
    }
}

#[test]
fn sqlite_replays_every_shared_ledger_scenario() {
    let fixture = document("ledger-scenarios.json");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 15);
    for case in fixture["cases"].as_array().unwrap() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("scenario.sqlite3");
        let session = SessionId::try_from("session").unwrap();
        let mut results = Vec::new();
        for operation in case["operations"].as_array().unwrap() {
            if let Some(commit) = operation.get("commit") {
                let result = SqliteLedger::open(&path).unwrap().commit(
                    session.clone(),
                    commit["expected"].as_u64().unwrap(),
                    commit["facts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(draft)
                        .collect(),
                );
                results.push(match result {
                    Ok(value) => format!(
                        "{}:{}:{}-{}",
                        match value.disposition {
                            CommitDisposition::Committed => "committed",
                            CommitDisposition::Replayed => "replayed",
                        },
                        value.session_version,
                        value.positions.first().unwrap(),
                        value.positions.last().unwrap(),
                    ),
                    Err(error) => format!("error:{}", error_code(error)),
                });
            } else if let Some(read) = operation.get("read") {
                let result = SqliteLedger::open(&path).unwrap().read_facts(
                    &session,
                    read["after"].as_u64().unwrap(),
                    read["through"].as_u64().unwrap(),
                    None,
                );
                results.push(match result {
                    Ok(facts) => format!(
                        "read:{}",
                        facts
                            .iter()
                            .map(|fact| fact.kind.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                    Err(error) => format!("error:{}", error_code(error)),
                });
            } else if operation.get("verify_corrupt").is_some() {
                let ledger = SqliteLedger::open(&path).unwrap();
                ledger
                    .connection_for_test()
                    .execute(
                        "UPDATE ledger_facts SET payload_sha256='0000000000000000000000000000000000000000000000000000000000000000' WHERE fact_id='f2'",
                        [],
                    )
                    .unwrap();
                drop(ledger);
                results.push(
                    match SqliteLedger::open(&path)
                        .and_then(|ledger| ledger.session_version(&session).map(|_| ()))
                    {
                        Err(error) => format!("error:{}", error_code(error)),
                        Ok(()) => "ok".into(),
                    },
                );
            }
        }
        let expected = &case["expected"];
        assert_eq!(
            results,
            expected["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{}",
            case["name"]
        );
        if case["name"] != "unknown-kind-preserved-and-corruption-rejected" {
            let ledger = SqliteLedger::open(&path).unwrap();
            assert_eq!(
                ledger.session_version(&session).unwrap(),
                Some(expected["version"].as_u64().unwrap())
            );
            let count: i64 = ledger
                .connection_for_test()
                .query_row("SELECT COUNT(*) FROM ledger_facts", [], |row| row.get(0))
                .unwrap();
            assert_eq!(
                count,
                expected["fact_count"].as_i64().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(
                ledger
                    .list_uncertain_model_requests(&session)
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>(),
                expected["uncertain"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                "{}",
                case["name"]
            );
        }
    }
}
