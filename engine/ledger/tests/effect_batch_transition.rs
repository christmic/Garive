use std::{fs, path::PathBuf};

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, LedgerError, LedgerState,
    SessionId, ToolInvocationId, TurnId,
};
use serde_json::{json, Value};

const FIRST_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const SECOND_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger/runtime-facts-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn payload(kind: &str, schema: u32) -> Value {
    fixture()["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["kind"] == kind
                && case["schema_version"].as_u64().unwrap_or(1) == u64::from(schema)
        })
        .unwrap()["payload"]
        .clone()
}

fn fact(id: &str, kind: &str, schema: u32, tool: Option<&str>, payload: Value) -> FactDraft {
    let session_only = kind == "session.opened";
    let turn_only = kind.starts_with("turn.");
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: (!session_only).then(|| TurnId::try_from("turn").unwrap()),
        execution_id: (!session_only && !turn_only)
            .then(|| ExecutionId::try_from("execution").unwrap()),
        model_request_id: None,
        tool_invocation_id: tool.map(|value| ToolInvocationId::try_from(value).unwrap()),
        kind: FactKind::new(kind).unwrap(),
        schema_version: schema,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-30T00:00:00Z".into(),
    }
}

fn content(value: &Value) -> Value {
    let canonical = CanonicalPayload::from_value(value).unwrap();
    json!({"digest": canonical.sha256(), "inline_utf8": canonical.as_json()})
}

fn prefix(include_second_authorization: bool) -> Vec<FactDraft> {
    let mut prepared_first = payload("effect.prepared", 2);
    let mut prepared_second = prepared_first.clone();
    prepared_second["prepared_digest"] = json!(SECOND_DIGEST);
    prepared_second["model_call_id"] = json!("call-v2-second");
    let authorized_first = payload("effect.authorized", 1);
    let mut authorized_second = authorized_first.clone();
    authorized_second["prepared_digest"] = json!(SECOND_DIGEST);
    authorized_second["grant_id"] = json!("grant-second");
    let mut facts = vec![
        fact("open", "session.opened", 1, None, json!({})),
        fact("turn", "turn.started", 1, None, payload("turn.started", 1)),
        fact(
            "execution",
            "execution.started",
            1,
            None,
            payload("execution.started", 1),
        ),
        fact(
            "prepared-first",
            "effect.prepared",
            2,
            Some("tool-first"),
            prepared_first.take(),
        ),
        fact(
            "authorized-first",
            "effect.authorized",
            1,
            Some("tool-first"),
            authorized_first,
        ),
        fact(
            "prepared-second",
            "effect.prepared",
            2,
            Some("tool-second"),
            prepared_second,
        ),
    ];
    if include_second_authorization {
        facts.push(fact(
            "authorized-second",
            "effect.authorized",
            1,
            Some("tool-second"),
            authorized_second,
        ));
    }
    facts
}

fn plan(steps: Value, max_buffered: u64) -> FactDraft {
    fact(
        "batch-plan",
        "execution.effect_batch_planned",
        1,
        None,
        json!({
            "plan_digest": FIRST_DIGEST,
            "conflict_graph_digest": FIRST_DIGEST,
            "ordered_prepared_digests": content(&json!([FIRST_DIGEST, SECOND_DIGEST])),
            "steps": content(&steps),
            "max_parallel_reads": 2,
            "max_buffered_result_bytes": max_buffered,
        }),
    )
}

fn started(id: &str, tool: &str, digest: &str, grant: &str) -> FactDraft {
    let mut value = payload("effect.started", 1);
    value["prepared_digest"] = json!(digest);
    value["grant_id"] = json!(grant);
    value["dispatch_attempt_id"] = json!(format!("attempt-{tool}"));
    fact(id, "effect.started", 1, Some(tool), value)
}

#[test]
fn planned_v2_effects_start_once_in_model_order() {
    let mut facts = prefix(true);
    facts.extend([
        plan(
            json!([{"kind":"parallel_read_group","intent_indexes":[0,1]}]),
            1024,
        ),
        started("started-first", "tool-first", FIRST_DIGEST, "grant"),
        started(
            "started-second",
            "tool-second",
            SECOND_DIGEST,
            "grant-second",
        ),
    ]);
    assert!(LedgerState::default()
        .commit(SessionId::try_from("session").unwrap(), 0, facts)
        .is_ok());
}

#[test]
fn missing_authorization_plan_and_reordered_starts_fail_closed() {
    let session = SessionId::try_from("session").unwrap();
    let steps = json!([{"kind":"parallel_read_group","intent_indexes":[0,1]}]);

    let mut missing_authorization = prefix(false);
    missing_authorization.push(plan(steps.clone(), 1024));
    assert_eq!(
        LedgerState::default().commit(session.clone(), 0, missing_authorization),
        Err(LedgerError::InvalidTransition)
    );

    let mut no_plan = prefix(true);
    no_plan.push(started("started", "tool-first", FIRST_DIGEST, "grant"));
    assert_eq!(
        LedgerState::default().commit(session.clone(), 0, no_plan),
        Err(LedgerError::InvalidTransition)
    );

    let mut reordered = prefix(true);
    reordered.extend([
        plan(steps, 1024),
        started(
            "started-second",
            "tool-second",
            SECOND_DIGEST,
            "grant-second",
        ),
    ]);
    assert_eq!(
        LedgerState::default().commit(session, 0, reordered),
        Err(LedgerError::InvalidTransition)
    );
}

#[test]
fn plan_coverage_and_buffer_charge_are_exact() {
    let session = SessionId::try_from("session").unwrap();
    for plan_fact in [
        plan(
            json!([{"kind":"parallel_read_group","intent_indexes":[0]}]),
            1024,
        ),
        plan(
            json!([{"kind":"parallel_read_group","intent_indexes":[0,1]}]),
            1023,
        ),
    ] {
        let mut facts = prefix(true);
        facts.push(plan_fact);
        assert_eq!(
            LedgerState::default().commit(session.clone(), 0, facts),
            Err(LedgerError::InvalidTransition)
        );
    }
}
