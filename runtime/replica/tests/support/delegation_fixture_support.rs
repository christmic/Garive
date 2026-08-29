use std::path::Path;

use garive_core::UsageSummary;
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, ExecutionId,
    FactDraft, FactId, FactKind, SessionId, TurnId,
};
use garive_llm::TokenCount;
use garive_multiagent::{
    complete_delegation_result, CancellationPolicy, ChildRequirement, ContentBinding,
    DelegationAllowance, DelegationBudget, DelegationConsumption, DelegationIntent,
    DelegationResult, DelegationResultContext, DelegationUsage, TokenUsageEvidence,
};
use garive_runtime::{
    plan_continue_turn, plan_delegation_authorization, plan_delegation_child_start,
    plan_delegation_child_terminal, plan_delegation_observation, plan_delegation_request,
    reconstruct_suspended_turn, ContinuationInput, ContinueTurnCommand,
    DelegationChildStartCommand, EffectiveRuntimeLimits, RuntimeCommandId, SqliteLedger,
};
use serde_json::json;

const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

pub fn run(database: &Path, checkpoint: &str) {
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = SqliteLedger::open(database).unwrap();
    ledger.commit(session.clone(),0,vec![fact("open","session.opened",None,None,json!({})),fact("parent","turn.started",Some("parent-turn"),None,json!({"command_id":"start","kind":"start","agent_instance_id":"parent-agent","definition_id":"parent","definition_revision":"1","snapshot_digest":EMPTY,"trusted_input_digest":EMPTY})),fact("parent-execution","execution.started",Some("parent-turn"),Some("parent-execution"),json!({"snapshot_digest":EMPTY,"through_position":0,"completed_iterations":0,"limits":{"max_iterations":4},"recovery_ordinal":0}))]).unwrap();
    let intent = intent();
    ledger
        .commit(
            session.clone(),
            1,
            vec![plan_delegation_request(&intent, "2026-08-29T00:00:01Z").unwrap()],
        )
        .unwrap();
    if checkpoint == "delegation_after_request" {
        return;
    }
    let (authorization, grant) = plan_delegation_authorization(
        &intent,
        "grant-1",
        "authority-1",
        0,
        0,
        &allowance(),
        "2026-08-29T00:00:02Z",
    )
    .unwrap();
    ledger.commit(session.clone(), 2, vec![grant]).unwrap();
    if checkpoint == "delegation_after_grant" {
        return;
    }
    let child = DelegationChildStartCommand {
        child_agent_instance_id: AgentInstanceId::try_from("child-agent").unwrap(),
        child_turn_id: TurnId::try_from("child-turn").unwrap(),
        child_execution_id: ExecutionId::try_from("child-execution").unwrap(),
        child_definition_id: AgentDefinitionId::try_from("reviewer").unwrap(),
        child_definition_revision: AgentDefinitionRevision::try_from("1").unwrap(),
        child_snapshot_digest: "c".repeat(64),
        resolved_objective: "Review child task.".into(),
        parent_completed_iterations: 0,
        parent_usage: UsageSummary {
            input_tokens: TokenCount::Unknown,
            output_tokens: TokenCount::Unknown,
            estimated: true,
        },
        child_limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(100),
            max_output_tokens: Some(50),
            deadline_budget_ms: Some(1_000),
        },
        through_position: 3,
        recorded_at: "2026-08-29T00:00:03Z".into(),
    };
    ledger
        .commit(
            session.clone(),
            3,
            plan_delegation_child_start(&intent, &authorization, &child).unwrap(),
        )
        .unwrap();
    if checkpoint == "delegation_after_child_start" {
        return;
    }
    let result = result(&intent);
    let terminal = vec![
        fact(
            "child-execution-done",
            "execution.completed",
            Some("child-turn"),
            Some("child-execution"),
            json!({"response":{"digest":EMPTY,"inline_utf8":""},"usage":{"input_tokens":{"kind":"known","value":10},"output_tokens":{"kind":"known","value":5},"source":"provider_reported"},"completed_iterations":1}),
        ),
        fact(
            "child-turn-done",
            "turn.completed",
            Some("child-turn"),
            None,
            json!({"execution_id":"child-execution","response":{"digest":EMPTY,"inline_utf8":""},"cumulative_usage":{"input_tokens":{"kind":"known","value":10},"output_tokens":{"kind":"known","value":5},"source":"provider_reported"}}),
        ),
    ];
    ledger
        .commit(
            session.clone(),
            4,
            plan_delegation_child_terminal(&intent, &result, terminal, "2026-08-29T00:00:04Z")
                .unwrap(),
        )
        .unwrap();
    if checkpoint == "delegation_after_child_terminal" {
        return;
    }
    ledger
        .commit(
            session.clone(),
            5,
            vec![plan_delegation_observation(&intent, &result, "2026-08-29T00:00:05Z").unwrap()],
        )
        .unwrap();
    if checkpoint == "delegation_after_observation" {
        return;
    }
    let state = reconstruct_suspended_turn(
        &ledger
            .load_turn(&TurnId::try_from("parent-turn").unwrap())
            .unwrap(),
    )
    .unwrap();
    let binding = result.result_binding().unwrap();
    let continuation = plan_continue_turn(
        &ContinueTurnCommand {
            command_id: RuntimeCommandId::new("continue").unwrap(),
            session_id: session.clone(),
            turn_id: TurnId::try_from("parent-turn").unwrap(),
            expected_suspension_id: state.suspension_id.clone(),
            expected_session_version: state.session_version,
            continuation_input: ContinuationInput::DelegationResult {
                delegation_id: "delegation-1".into(),
                result_id: "result-1".into(),
                content: binding.inline_utf8().unwrap().into(),
            },
            interaction: None,
            recorded_at: "2026-08-29T00:00:06Z".into(),
        },
        &state,
    )
    .unwrap();
    ledger
        .commit(session, state.session_version, continuation.facts)
        .unwrap();
    assert_eq!(checkpoint, "delegation_after_continuation");
}

fn intent() -> DelegationIntent {
    DelegationIntent::new("delegation-1","parent-agent","parent-turn","parent-execution",ChildRequirement::definition("reviewer","1").unwrap(),ContentBinding::from_inline("Review child task."),Vec::new(),ContentBinding::from_inline("{\"additionalProperties\":false,\"properties\":{\"answer\":{\"type\":\"string\"}},\"required\":[\"answer\"],\"type\":\"object\"}"),DelegationBudget { max_child_turns:1,max_child_executions:2,max_iterations:4,max_input_tokens:100,max_output_tokens:50,deadline_budget_ms:1_000,max_depth:2,max_objective_bytes:64,max_input_evidence:1,max_result_schema_bytes:256,max_result_bytes:64,max_result_evidence:1 },CancellationPolicy::CancelWithParent,3).unwrap()
}
fn allowance() -> DelegationAllowance {
    DelegationAllowance {
        remaining_child_turns: 1,
        remaining_child_executions: 2,
        remaining_iterations: 4,
        remaining_input_tokens: 100,
        remaining_output_tokens: 50,
        remaining_elapsed_ms: 1_000,
        max_depth: 2,
        max_objective_bytes: 64,
        max_input_evidence: 1,
        max_result_schema_bytes: 256,
        max_result_bytes: 64,
        max_result_evidence: 1,
    }
}
fn result(intent: &DelegationIntent) -> DelegationResult {
    let content = "{\"answer\":\"done\"}";
    complete_delegation_result(
        intent,
        DelegationResultContext {
            result_id: "result-1".into(),
            delegation_id: "delegation-1".into(),
            grant_id: "grant-1".into(),
            child_agent_instance_id: "child-agent".into(),
            child_turn_id: "child-turn".into(),
            child_snapshot_digest: "c".repeat(64),
            usage: DelegationUsage {
                input_tokens: TokenUsageEvidence::Known { value: 10 },
                output_tokens: TokenUsageEvidence::Known { value: 5 },
            },
            consumption: DelegationConsumption {
                child_turns: 1,
                child_executions: 1,
                completed_iterations: 1,
                elapsed_ms: 100,
            },
        },
        ContentBinding::from_inline(content),
        content,
        Vec::new(),
    )
    .unwrap()
}
fn fact(
    id: &str,
    kind: &str,
    turn: Option<&str>,
    execution: Option<&str>,
    payload: serde_json::Value,
) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: turn.map(|v| TurnId::try_from(v).unwrap()),
        execution_id: execution.map(|v| ExecutionId::try_from(v).unwrap()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}
