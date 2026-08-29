use std::{fs, path::PathBuf};

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload,
    CommitDisposition, FactDraft, FactId, FactKind, SessionId,
};
use garive_runtime::{
    commit_planned_turn, plan_cancel_turn, plan_continue_turn, plan_recovery_restart,
    plan_start_turn, reconstruct_suspended_turn, select_runtime_recovery, CancelReason,
    CancelTurnCommand, ContinueTurnCommand, EffectRecoveryPosition, EffectiveRuntimeLimits,
    ExecutionRecoveryPosition, ModelRecoveryPosition, RecoveryRestartCommand, RuntimeCommandError,
    RuntimeCommandId, RuntimeRecoveryAction, RuntimeRecoverySnapshot, SqliteLedger,
    StartTurnCommand,
};
use serde_json::{json, Value};
use tempfile::tempdir;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/durable-runtime-turn.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn start_command(input: &str, command_id: &str) -> StartTurnCommand {
    let value = fixture();
    let start = &value["start"];
    StartTurnCommand {
        command_id: RuntimeCommandId::new(command_id).unwrap(),
        session_id: SessionId::try_from(start["session_id"].as_str().unwrap()).unwrap(),
        agent_instance_id: AgentInstanceId::try_from(start["agent_instance_id"].as_str().unwrap())
            .unwrap(),
        definition_id: AgentDefinitionId::try_from(start["definition_id"].as_str().unwrap())
            .unwrap(),
        definition_revision: AgentDefinitionRevision::try_from(
            start["definition_revision"].as_str().unwrap(),
        )
        .unwrap(),
        snapshot_digest: start["snapshot_digest"].as_str().unwrap().to_owned(),
        trusted_input: input.to_owned(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(1_024),
            max_output_tokens: Some(512),
            deadline_budget_ms: Some(30_000),
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("session-open").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn runtime_payload(kind: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger/runtime-facts-v1.json");
    let value: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    value["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["kind"].as_str() == Some(kind))
        .unwrap()["payload"]
        .clone()
}

fn suspension_fact(
    id: &str,
    kind: &str,
    turn: &garive_ledger::TurnId,
    execution: Option<&garive_ledger::ExecutionId>,
) -> FactDraft {
    let mut payload = runtime_payload(kind);
    if kind == "turn.suspended" {
        payload.as_object_mut().unwrap().insert(
            "execution_id".into(),
            Value::String(execution.unwrap().as_str().into()),
        );
    }
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: Some(turn.clone()),
        execution_id: execution
            .cloned()
            .filter(|_| kind.starts_with("execution.")),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

#[test]
fn start_plan_matches_the_frozen_transaction_contract() {
    let fixture = fixture();
    let command = start_command("hello", "command-start");
    let plan = plan_start_turn(&command, 1).unwrap();
    let kinds: Vec<_> = plan.facts.iter().map(|fact| fact.kind.as_str()).collect();
    let expected: Vec<_> = fixture["start"]["expected_facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(kinds, expected);
    assert_eq!(plan.facts[2].execution_id, plan.execution_id);
    assert!(plan
        .facts
        .iter()
        .all(|fact| fact.turn_id.as_ref() == Some(&plan.turn_id)));
}

#[test]
fn sqlite_command_ids_replay_or_conflict_without_partial_append() {
    let directory = tempdir().unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("runtime.sqlite3")).unwrap();
    let command = start_command("hello", "command-start");
    let session = command.session_id.clone();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let plan = plan_start_turn(&command, 1).unwrap();
    let committed = commit_planned_turn(&mut ledger, session.clone(), 1, &plan).unwrap();
    assert_eq!(committed.disposition, CommitDisposition::Committed);
    assert_eq!(committed.positions, vec![2, 3, 4]);

    let replayed = commit_planned_turn(&mut ledger, session.clone(), 1, &plan).unwrap();
    assert_eq!(replayed.disposition, CommitDisposition::Replayed);
    let changed = plan_start_turn(&start_command("changed", "command-start"), 1).unwrap();
    assert_eq!(
        commit_planned_turn(&mut ledger, session.clone(), 2, &changed),
        Err(RuntimeCommandError::CommandConflict)
    );
    let fresh = plan_start_turn(&start_command("hello", "new-command"), 4).unwrap();
    assert_eq!(
        commit_planned_turn(&mut ledger, session.clone(), 1, &fresh),
        Err(RuntimeCommandError::ConcurrentModification)
    );

    let cancel = plan_cancel_turn(&CancelTurnCommand {
        command_id: RuntimeCommandId::new("cancel-command").unwrap(),
        session_id: session.clone(),
        turn_id: plan_start_turn(&command, 1).unwrap().turn_id,
        reason: CancelReason::User,
        requested_through_position: 4,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    })
    .unwrap();
    assert_eq!(
        commit_planned_turn(&mut ledger, session.clone(), 2, &cancel)
            .unwrap()
            .positions,
        vec![5]
    );
    assert_eq!(ledger.session_version(&session).unwrap(), Some(3));
}

#[test]
fn invalid_constructed_limits_and_clock_fail_before_a_fact_exists() {
    let mut command = start_command("hello", "invalid");
    command.limits.max_iterations = 0;
    assert!(plan_start_turn(&command, 0).is_err());
    command.limits.max_iterations = 1;
    command.recorded_at = "today".into();
    assert!(plan_start_turn(&command, 0).is_err());
}

#[test]
fn continuation_reopens_a_suspended_turn_with_a_fresh_execution() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("continue.sqlite3");
    let mut ledger = SqliteLedger::open(&path).unwrap();
    let start = start_command("hello", "command-start");
    let session = start.session_id.clone();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let started = plan_start_turn(&start, 1).unwrap();
    let prior_execution = started.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, started.facts).unwrap();
    ledger
        .commit(
            session.clone(),
            2,
            vec![
                suspension_fact(
                    "execution-suspended",
                    "execution.suspended",
                    &started.turn_id,
                    Some(&prior_execution),
                ),
                suspension_fact(
                    "turn-suspended",
                    "turn.suspended",
                    &started.turn_id,
                    Some(&prior_execution),
                ),
            ],
        )
        .unwrap();
    drop(ledger);
    let mut ledger = SqliteLedger::open(&path).unwrap();
    let state = reconstruct_suspended_turn(&ledger.load_turn(&started.turn_id).unwrap()).unwrap();
    assert_eq!((state.session_version, state.through_position), (3, 6));
    let command = ContinueTurnCommand {
        command_id: RuntimeCommandId::new("continue-command").unwrap(),
        session_id: session.clone(),
        turn_id: started.turn_id.clone(),
        expected_suspension_id: "suspension".into(),
        expected_session_version: 3,
        continuation_input: "approved".into(),
        interaction: None,
        recorded_at: "2026-08-29T00:00:02Z".into(),
    };
    let continued = plan_continue_turn(&command, &state).unwrap();
    assert_ne!(continued.execution_id, Some(prior_execution));
    assert_eq!(
        ledger
            .commit(session, 3, continued.facts)
            .unwrap()
            .positions,
        vec![7, 8, 9]
    );
    let mut stale = command.clone();
    stale.expected_session_version = 2;
    assert_eq!(
        plan_continue_turn(&stale, &state),
        Err(RuntimeCommandError::ConcurrentModification)
    );
    stale.expected_session_version = 3;
    stale.expected_suspension_id = "other".into();
    assert_eq!(
        plan_continue_turn(&stale, &state),
        Err(RuntimeCommandError::ContinuationMismatch)
    );
    stale.expected_suspension_id = "suspension".into();
    stale.session_id = SessionId::try_from("other-session").unwrap();
    assert_eq!(
        plan_continue_turn(&stale, &state),
        Err(RuntimeCommandError::ContinuationMismatch)
    );
}

#[test]
fn recovery_reducer_consumes_every_frozen_restart_case() {
    let fixture = fixture();
    let cases = fixture["recovery_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    for case in cases {
        let execution = match case["execution"].as_str().unwrap() {
            "active" => ExecutionRecoveryPosition::Active,
            "suspended" => ExecutionRecoveryPosition::Suspended,
            "terminal" => ExecutionRecoveryPosition::Terminal,
            value => panic!("unknown execution position {value}"),
        };
        let model = match case["model"].as_str().unwrap() {
            "none" => ModelRecoveryPosition::None,
            "prepared" => ModelRecoveryPosition::Prepared,
            "started" => ModelRecoveryPosition::Started,
            "terminal" => ModelRecoveryPosition::Terminal,
            value => panic!("unknown model position {value}"),
        };
        let effect = match case["effect"].as_str().unwrap() {
            "none" => EffectRecoveryPosition::None,
            "prepared" => EffectRecoveryPosition::Prepared,
            "started" => EffectRecoveryPosition::Started,
            "receipt" => EffectRecoveryPosition::Receipt,
            "uncertain" => EffectRecoveryPosition::Uncertain,
            "interaction_requested" => EffectRecoveryPosition::InteractionRequested,
            "terminal" => EffectRecoveryPosition::Terminal,
            value => panic!("unknown effect position {value}"),
        };
        let expected = match case["expected"].as_str().unwrap() {
            "abandon_and_restart" => RuntimeRecoveryAction::AbandonAndRestart,
            "classify_model_uncertain" => RuntimeRecoveryAction::ClassifyModelUncertain,
            "classify_effect_uncertain" => RuntimeRecoveryAction::ClassifyEffectUncertain,
            "recover_receipt_terminal" => RuntimeRecoveryAction::RecoverReceiptTerminal,
            "await_continuation" => RuntimeRecoveryAction::AwaitContinuation,
            "return_committed_terminal" => RuntimeRecoveryAction::ReturnCommittedTerminal,
            "fail_recovery_bound" => RuntimeRecoveryAction::FailRecoveryBound,
            value => panic!("unknown recovery action {value}"),
        };
        assert_eq!(
            select_runtime_recovery(RuntimeRecoverySnapshot {
                execution,
                model,
                effect,
                recovery_ordinal: case["recovery_ordinal"].as_u64().unwrap_or(0),
                max_recoveries: case["max_recoveries"].as_u64().unwrap_or(3),
            }),
            expected,
            "{}",
            case["name"]
        );
    }
}

#[test]
fn restart_atomically_abandons_the_lost_execution_and_starts_one_replacement() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("recovery.sqlite3");
    let start = start_command("hello", "command-start");
    let session = start.session_id.clone();
    let started = plan_start_turn(&start, 1).unwrap();
    let lost = started.execution_id.clone().unwrap();
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        ledger
            .commit(session.clone(), 0, vec![open_session()])
            .unwrap();
        ledger.commit(session.clone(), 1, started.facts).unwrap();
    }
    let command = RecoveryRestartCommand {
        recovery_id: RuntimeCommandId::new("recovery-1").unwrap(),
        turn_id: started.turn_id.clone(),
        lost_execution_id: lost.clone(),
        snapshot_digest: start.snapshot_digest.clone(),
        last_safe_position: 4,
        completed_iterations: 0,
        recovery_ordinal: 1,
        limits: start.limits,
        recorded_at: "2026-08-29T00:00:02Z".into(),
    };
    let recovery = plan_recovery_restart(&command).unwrap();
    let replacement = recovery.execution_id.clone().unwrap();
    assert_ne!(lost, replacement);
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        assert_eq!(
            ledger
                .commit(session.clone(), 2, recovery.facts)
                .unwrap()
                .positions,
            vec![5, 6]
        );
    }
    let ledger = SqliteLedger::open(&path).unwrap();
    let snapshot = ledger.load_turn(&started.turn_id).unwrap();
    assert_eq!(snapshot.session_version, 3);
    assert_eq!(
        snapshot
            .facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "execution.abandoned")
            .count(),
        1
    );
    assert_eq!(
        snapshot
            .facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "execution.started")
            .count(),
        2
    );
}
