use std::{collections::BTreeSet, fs, path::PathBuf};

use garive_ledger::{
    CanonicalPayload, CommitDisposition, DurableFact, ExecutionId, FactDraft, FactId, FactKind,
    LedgerError, LedgerState, ModelRequestId, SessionId, ToolInvocationId, TurnId,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger/ledger-scenarios.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn draft(value: &Value) -> FactDraft {
    let payload_value = value
        .get("payload")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
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
        payload: CanonicalPayload::from_value(&payload_value).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn render_commit(result: Result<garive_ledger::CommitResult, LedgerError>) -> String {
    match result {
        Ok(value) => {
            let disposition = match value.disposition {
                CommitDisposition::Committed => "committed",
                CommitDisposition::Replayed => "replayed",
            };
            format!(
                "{disposition}:{}:{}-{}",
                value.session_version,
                value.positions.first().unwrap(),
                value.positions.last().unwrap()
            )
        }
        Err(error) => format!("error:{}", error.code()),
    }
}

fn render_read(result: Result<Vec<DurableFact>, LedgerError>) -> String {
    match result {
        Ok(facts) => format!(
            "read:{}",
            facts
                .iter()
                .map(|fact| fact.kind.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ),
        Err(error) => format!("error:{}", error.code()),
    }
}

#[test]
fn rust_consumes_every_ledger_scenario() {
    let document = fixture();
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    for case in cases {
        let session_id = SessionId::try_from("session").unwrap();
        let mut ledger = LedgerState::default();
        let mut results = Vec::new();
        for operation in case["operations"].as_array().unwrap() {
            if let Some(commit) = operation.get("commit") {
                let drafts = commit["facts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(draft)
                    .collect();
                results.push(render_commit(ledger.commit(
                    session_id.clone(),
                    commit["expected"].as_u64().unwrap(),
                    drafts,
                )));
            } else if let Some(read) = operation.get("read") {
                results.push(render_read(ledger.read_facts(
                    &session_id,
                    read["after"].as_u64().unwrap(),
                    read["through"].as_u64().unwrap(),
                    None,
                )));
            } else if let Some(corrupt) = operation.get("verify_corrupt") {
                let mut fact = ledger
                    .fact_at(&session_id, corrupt["position"].as_u64().unwrap())
                    .unwrap();
                fact.payload = fact.payload.with_digest_for_corruption_test("00");
                results.push(match fact.verify() {
                    Ok(()) => "ok".into(),
                    Err(error) => format!("error:{}", error.code()),
                });
            } else {
                panic!("unknown operation in {}", case["name"]);
            }
        }
        let expected = &case["expected"];
        let expected_results: Vec<_> = expected["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(results, expected_results, "{}", case["name"]);
        assert_eq!(
            ledger.session_version(&session_id),
            Some(expected["version"].as_u64().unwrap()),
            "{}",
            case["name"]
        );
        assert_eq!(
            ledger.fact_count(&session_id),
            expected["fact_count"].as_u64().unwrap() as usize,
            "{}",
            case["name"]
        );
        let uncertain: Vec<_> = ledger
            .list_uncertain_model_requests(&session_id)
            .unwrap()
            .iter()
            .map(|value| value.as_str().to_owned())
            .collect();
        let expected_uncertain: Vec<_> = expected["uncertain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(uncertain, expected_uncertain, "{}", case["name"]);
    }
}

#[test]
fn canonical_payload_is_cross_language_stable() {
    let value: Value = serde_json::from_str(r#"{"z":[2,1],"a":"蟹","escaped":"\n"}"#).unwrap();
    let payload = CanonicalPayload::from_value(&value).unwrap();
    assert_eq!(payload.as_json(), r#"{"a":"蟹","escaped":"\n","z":[2,1]}"#);
    assert_eq!(payload.sha256().len(), 64);
    assert!(CanonicalPayload::from_value(&serde_json::json!(1.5)).is_err());
    let kinds = BTreeSet::from([FactKind::new("session.opened").unwrap()]);
    assert_eq!(kinds.len(), 1);
}
