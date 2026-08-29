use garive_ledger::{
    CanonicalPayload, DurableFact, ExecutionId, FactId, FactKind, SessionId, TurnId, TurnSnapshot,
};
use garive_runtime::{
    plan_continue_turn, reconstruct_suspended_turn, ContinuationInput, ContinueTurnCommand,
    RuntimeCommandError, RuntimeCommandId,
};
use serde_json::{json, Value};

const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const RESULT: &str = "f6a214f7a5fcda0c2cee9660b7fc29f5649e3c68aad48e20e950137c98913a68";

fn fact(position: u64, kind: &str, execution: bool, payload: Value) -> DurableFact {
    DurableFact {
        fact_id: FactId::try_from(format!("fact-{position}").as_str()).unwrap(),
        session_id: SessionId::try_from("session").unwrap(),
        position,
        turn_id: Some(TurnId::try_from("parent-turn").unwrap()),
        execution_id: execution.then(|| ExecutionId::try_from("parent-execution").unwrap()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn snapshot(observed: bool) -> TurnSnapshot {
    let mut facts = vec![
        fact(
            1,
            "turn.started",
            false,
            json!({"command_id":"start","kind":"start","agent_instance_id":"parent-agent","definition_id":"definition","definition_revision":"1","snapshot_digest":EMPTY,"trusted_input_digest":EMPTY}),
        ),
        fact(
            2,
            "execution.started",
            true,
            json!({"snapshot_digest":EMPTY,"through_position":1,"completed_iterations":0,"limits":{"max_iterations":4},"recovery_ordinal":0}),
        ),
        fact(
            3,
            "delegation.requested",
            true,
            json!({"delegation_id":"delegation-1","parent_agent_instance_id":"parent-agent","intent":{"digest":EMPTY,"reference":"intent"},"intent_digest":EMPTY,"through_position":2}),
        ),
        fact(
            4,
            "delegation.authorized",
            true,
            json!({"delegation_id":"delegation-1","grant_id":"grant-1","intent_digest":EMPTY,"reserved_budget":{"max_child_turns":1,"max_child_executions":2,"max_iterations":4,"max_input_tokens":100,"max_output_tokens":50,"deadline_budget_ms":1000,"max_depth":2,"max_objective_bytes":64,"max_input_evidence":1,"max_result_schema_bytes":128,"max_result_bytes":64,"max_result_evidence":1},"authority_revision":"authority"}),
        ),
        fact(
            5,
            "execution.suspended",
            true,
            json!({"suspension_id":"delegation-suspension","reason":"delegation_pending","continuation":{"digest":EMPTY,"inline_utf8":""},"usage":{"input_tokens":{"kind":"unknown"},"output_tokens":{"kind":"unknown"},"source":"estimated"},"completed_iterations":1}),
        ),
        fact(
            6,
            "turn.suspended",
            false,
            json!({"suspension_id":"delegation-suspension","execution_id":"parent-execution","reason":"delegation_pending","continuation":{"digest":EMPTY,"inline_utf8":""},"cumulative_usage":{"input_tokens":{"kind":"unknown"},"output_tokens":{"kind":"unknown"},"source":"estimated"}}),
        ),
        fact(
            7,
            "delegation.child_started",
            true,
            json!({"delegation_id":"delegation-1","grant_id":"grant-1","suspension_id":"delegation-suspension","child_agent_instance_id":"child-agent","child_turn_id":"child-turn","child_snapshot_digest":EMPTY}),
        ),
        fact(
            8,
            "delegation.child_terminal",
            true,
            json!({"delegation_id":"delegation-1","grant_id":"grant-1","result_id":"result-1","suspension_id":"delegation-suspension","child_agent_instance_id":"child-agent","child_turn_id":"child-turn","result":{"digest":RESULT,"inline_utf8":"result"},"result_digest":RESULT}),
        ),
    ];
    if observed {
        facts.push(fact(9, "delegation.observed", true, json!({"delegation_id":"delegation-1","grant_id":"grant-1","result_id":"result-1","suspension_id":"delegation-suspension","result_digest":RESULT})));
    }
    TurnSnapshot {
        facts,
        session_version: if observed { 4 } else { 3 },
        through_position: if observed { 9 } else { 8 },
    }
}

fn command(version: u64, content: &str) -> ContinueTurnCommand {
    ContinueTurnCommand {
        command_id: RuntimeCommandId::new("continue").unwrap(),
        session_id: SessionId::try_from("session").unwrap(),
        turn_id: TurnId::try_from("parent-turn").unwrap(),
        expected_suspension_id: "delegation-suspension".into(),
        expected_session_version: version,
        continuation_input: ContinuationInput::DelegationResult {
            delegation_id: "delegation-1".into(),
            result_id: "result-1".into(),
            content: content.into(),
        },
        interaction: None,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

#[test]
fn observed_result_is_the_only_delegation_continuation() {
    let pending = reconstruct_suspended_turn(&snapshot(false)).unwrap();
    assert!(!pending.delegation.as_ref().unwrap().observed);
    assert_eq!(
        plan_continue_turn(&command(3, "result"), &pending),
        Err(RuntimeCommandError::ContinuationMismatch)
    );

    let ready = reconstruct_suspended_turn(&snapshot(true)).unwrap();
    let binding = ready.delegation.as_ref().unwrap();
    assert_eq!(
        (
            binding.delegation_id.as_str(),
            binding.result_digest.as_deref(),
            binding.observed
        ),
        ("delegation-1", Some(RESULT), true)
    );
    assert_eq!(
        plan_continue_turn(&command(4, "changed"), &ready),
        Err(RuntimeCommandError::ContinuationMismatch)
    );
    let planned = plan_continue_turn(&command(4, "result"), &ready).unwrap();
    let input: Value = serde_json::from_str(planned.facts[0].payload.as_json()).unwrap();
    assert_eq!(
        (
            input["input_kind"].as_str(),
            input["content"]["digest"].as_str()
        ),
        (Some("delegation_result"), Some(RESULT))
    );
}
