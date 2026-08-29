use garive_ledger::{
    CanonicalPayload, CommitDisposition, ExecutionId, FactDraft, FactId, FactKind, LedgerError,
    LedgerState, SessionId, TurnId,
};
use serde_json::{json, Value};

mod common;

const EMPTY_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const RESULT_DIGEST: &str = "83497274cc7affcc460bca7452c14d5e72eaa019a33055df2bc39cd9a5202774";

fn fact(
    id: &str,
    kind: &str,
    turn: Option<&str>,
    execution: Option<&str>,
    payload: Value,
) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: turn.map(|value| TurnId::try_from(value).unwrap()),
        execution_id: execution.map(|value| ExecutionId::try_from(value).unwrap()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn fixture(id: &str, kind: &str, turn: Option<&str>, execution: Option<&str>) -> FactDraft {
    fact(id, kind, turn, execution, common::runtime_payload(kind))
}

fn establish_authorized(ledger: &mut LedgerState, session: &SessionId) {
    let opened = fixture("open", "session.opened", None, None);
    let parent = fixture("parent", "turn.started", Some("turn"), None);
    let execution = fixture(
        "parent-execution",
        "execution.started",
        Some("turn"),
        Some("execution"),
    );
    ledger
        .commit(session.clone(), 0, vec![opened, parent, execution])
        .unwrap();
    let mut requested = common::runtime_payload("delegation.requested");
    requested["parent_agent_instance_id"] = json!("agent");
    ledger
        .commit(
            session.clone(),
            1,
            vec![fact(
                "requested",
                "delegation.requested",
                Some("turn"),
                Some("execution"),
                requested,
            )],
        )
        .unwrap();
    ledger
        .commit(
            session.clone(),
            2,
            vec![fixture(
                "authorized",
                "delegation.authorized",
                Some("turn"),
                Some("execution"),
            )],
        )
        .unwrap();
}

fn child_start_batch(include_binding: bool) -> Vec<FactDraft> {
    let mut execution_suspended = common::runtime_payload("execution.suspended");
    execution_suspended["suspension_id"] = json!("delegation-suspension-1");
    execution_suspended["reason"] = json!("delegation_pending");
    let mut turn_suspended = common::runtime_payload("turn.suspended");
    turn_suspended["suspension_id"] = json!("delegation-suspension-1");
    turn_suspended["reason"] = json!("delegation_pending");
    let mut child = common::runtime_payload("turn.started");
    child["agent_instance_id"] = json!("child-agent");
    child["snapshot_digest"] = json!("c".repeat(64));
    let mut batch = vec![
        fact(
            "execution-suspended",
            "execution.suspended",
            Some("turn"),
            Some("execution"),
            execution_suspended,
        ),
        fact(
            "turn-suspended",
            "turn.suspended",
            Some("turn"),
            None,
            turn_suspended,
        ),
        fact(
            "child-start",
            "turn.started",
            Some("child-turn"),
            None,
            child,
        ),
    ];
    if include_binding {
        batch.push(fixture(
            "child-bound",
            "delegation.child_started",
            Some("turn"),
            Some("execution"),
        ));
    }
    batch
}

#[test]
fn delegation_requires_atomic_child_start_terminal_and_observed_continuation() {
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = LedgerState::default();
    establish_authorized(&mut ledger, &session);

    let mut missing_binding = ledger.clone();
    assert_eq!(
        missing_binding.commit(session.clone(), 3, child_start_batch(false)),
        Err(LedgerError::InvalidTransition)
    );
    assert_eq!(missing_binding.session_version(&session), Some(3));

    assert_eq!(
        ledger
            .commit(session.clone(), 3, child_start_batch(true))
            .unwrap()
            .disposition,
        CommitDisposition::Committed
    );
    let mut child_turn_terminal = common::runtime_payload("turn.completed");
    child_turn_terminal["execution_id"] = json!("child-execution");
    let terminal = vec![
        fact(
            "child-execution",
            "execution.started",
            Some("child-turn"),
            Some("child-execution"),
            json!({"snapshot_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","through_position":0,"completed_iterations":0,"limits":{"max_iterations":1},"recovery_ordinal":0}),
        ),
        fixture(
            "child-execution-done",
            "execution.completed",
            Some("child-turn"),
            Some("child-execution"),
        ),
        fact(
            "child-turn-done",
            "turn.completed",
            Some("child-turn"),
            None,
            child_turn_terminal,
        ),
        fixture(
            "delegation-terminal",
            "delegation.child_terminal",
            Some("turn"),
            Some("execution"),
        ),
    ];
    let mut missing_terminal = ledger.clone();
    assert_eq!(
        missing_terminal.commit(session.clone(), 4, terminal[..3].to_vec()),
        Err(LedgerError::InvalidTransition),
    );
    ledger.commit(session.clone(), 4, terminal).unwrap();
    let premature_input = fact(
        "premature-result",
        "turn.input",
        Some("turn"),
        None,
        json!({"input_kind":"delegation_result","content":{"digest":RESULT_DIGEST,"reference":"fixture:delegation-result-1"},"suspension_id":"delegation-suspension-1"}),
    );
    assert_eq!(
        ledger
            .clone()
            .commit(session.clone(), 5, vec![premature_input]),
        Err(LedgerError::InvalidTransition),
    );
    ledger
        .commit(
            session.clone(),
            5,
            vec![fixture(
                "observed",
                "delegation.observed",
                Some("turn"),
                Some("execution"),
            )],
        )
        .unwrap();

    let continuation_input = fact(
        "result-input",
        "turn.input",
        Some("turn"),
        None,
        json!({
            "input_kind":"delegation_result", "content":{"digest":RESULT_DIGEST,"reference":"fixture:delegation-result-1"},
            "suspension_id":"delegation-suspension-1"
        }),
    );
    let continued = fact(
        "continued",
        "turn.started",
        Some("turn"),
        None,
        json!({
            "command_id":"continue", "kind":"continue", "agent_instance_id":"agent",
            "definition_id":"definition", "definition_revision":"revision", "snapshot_digest":EMPTY_DIGEST,
            "trusted_input_digest":EMPTY_DIGEST, "prior_suspension_id":"delegation-suspension-1", "expected_session_version":6
        }),
    );
    ledger
        .commit(session, 6, vec![continuation_input, continued])
        .unwrap();
}
