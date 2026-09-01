use std::sync::Arc;

use garive_core::{AgentOutcome, ExecutionReport, UsageSummary};
use garive_ledger::{AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, SessionId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    bind_completed_plan_proposal_result, commit_planned_turn, plan_core_terminal,
    plan_start_plan_proposal_execution, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, RuntimeCommandId, SqliteLedger,
    StartPlanProposalExecutionCommand, StartTurnCommand, TurnDispatchError, TurnDispatcher,
};
use tempfile::tempdir;

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-09-01T00:00:00Z".into()
    }
}
struct Sink;
impl TurnDispatcher for Sink {
    fn dispatch(&self, _: &garive_runtime::CommittedTurn) -> Result<(), TurnDispatchError> {
        Ok(())
    }
}

#[test]
fn completed_planner_result_is_bound_once_from_ledger() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("planner-result.db");
    let limits = EffectiveRuntimeLimits {
        max_iterations: 1,
        max_input_tokens: Some(100),
        max_output_tokens: Some(100),
        deadline_budget_ms: None,
    };
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "planner".into(),
            definition_revision: "1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "planner".into(),
            public_capabilities: Vec::new(),
            runtime_limits: limits,
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4096,
            event_batch_size: 16,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        Arc::new(Sink),
    )
    .unwrap();
    let session = host.create_session("create", "planner").unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("planner-request").unwrap(),
        session_id: SessionId::try_from(session.session_id.as_str()).unwrap(),
        agent_instance_id: AgentInstanceId::try_from(session.agent_instance_id.as_str()).unwrap(),
        definition_id: AgentDefinitionId::try_from("planner").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("1").unwrap(),
        snapshot_digest: "a".repeat(64),
        trusted_input: "plan".into(),
        limits,
        recorded_at: "2026-09-01T00:00:01Z".into(),
    };
    let planned = plan_start_plan_proposal_execution(
        &StartPlanProposalExecutionCommand {
            start,
            goal_id: "goal-1".into(),
            goal_revision: 1,
            goal_definition_digest: "b".repeat(64),
            expected_session_version: 1,
            proposer_reference: "planner-v1".into(),
            output_schema_digest: "c".repeat(64),
        },
        1,
    )
    .unwrap();
    let turn_id = planned.turn_id.clone();
    let execution_id = planned.execution_id.clone().unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    commit_planned_turn(
        &mut ledger,
        SessionId::try_from(session.session_id.as_str()).unwrap(),
        1,
        &planned,
    )
    .unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(2),
        estimated: false,
    };
    let terminals = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn_id.clone(),
            execution_id,
            recorded_at: "2026-09-01T00:00:02Z".into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: "{\"steps\":[]}".into(),
                }],
                usage,
            },
            completed_iterations: 1,
            usage,
        },
    )
    .unwrap();
    ledger
        .commit(
            SessionId::try_from(session.session_id.as_str()).unwrap(),
            2,
            terminals,
        )
        .unwrap();
    let bound = bind_completed_plan_proposal_result(
        &mut ledger,
        &SessionId::try_from(session.session_id.as_str()).unwrap(),
        &turn_id,
        3,
        "2026-09-01T00:00:03Z",
    )
    .unwrap();
    assert_eq!(bound.binding_position, 8);
    assert!(bound.response_items_json.contains("{\\\"steps\\\":[]}"));
    let replay = bind_completed_plan_proposal_result(
        &mut ledger,
        &SessionId::try_from(session.session_id.as_str()).unwrap(),
        &turn_id,
        3,
        "2026-09-01T00:00:03Z",
    )
    .unwrap();
    assert_eq!(replay, bound);
}
