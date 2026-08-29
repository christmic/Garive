use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};

use garive_ledger::{SessionId, TurnId, TurnSnapshot};
use garive_runtime::{
    derive_runtime_recovery, plan_recovery_action_facts, reconstruct_schedule_state,
    select_runtime_recovery, ExecutionLeaseError, ExecutionLeaseRequest, RuntimeRecoveryAction,
    SqliteLedger,
};
use tempfile::{tempdir, TempDir};

const CHECKPOINTS: &[&str] = &[
    "before_start",
    "after_start",
    "iteration_started",
    "cancel_requested",
    "model_prepared",
    "model_started",
    "model_completed",
    "effect_prepared",
    "effect_authorized",
    "effect_started",
    "effect_receipt",
    "effect_completed",
    "effect_observation",
    "interaction_requested",
    "interaction_resolved",
    "before_terminal",
    "after_terminal",
];

const SCHEDULE_CHECKPOINTS: &[&str] = &[
    "scheduler_before_claim",
    "scheduler_after_claim",
    "scheduler_after_dispatch",
];

const DELEGATION_CHECKPOINTS: &[&str] = &[
    "delegation_after_request",
    "delegation_after_grant",
    "delegation_after_child_start",
    "delegation_after_child_terminal",
    "delegation_after_observation",
    "delegation_after_continuation",
];

#[test]
fn killed_delegation_recovers_every_durable_boundary() {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (index, checkpoint) in DELEGATION_CHECKPOINTS.iter().enumerate() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("delegation-kill.sqlite3");
        let mut child = Command::new(env!("CARGO_BIN_EXE_garive-runtime-crash-fixture"))
            .args([
                database.to_str().unwrap(),
                repository.to_str().unwrap(),
                checkpoint,
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "READY");
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let ledger = SqliteLedger::open(&database).unwrap();
        for (kind, expected) in [
            ("delegation.requested", 1),
            ("delegation.authorized", i64::from(index >= 1)),
            ("delegation.child_started", i64::from(index >= 2)),
            ("delegation.child_terminal", i64::from(index >= 3)),
            ("delegation.observed", i64::from(index >= 4)),
        ] {
            let count: i64 = ledger
                .connection_for_test()
                .query_row(
                    "SELECT COUNT(*) FROM ledger_facts WHERE kind=?1",
                    [kind],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, expected, "{checkpoint}:{kind}");
        }
        let result_inputs:i64=ledger.connection_for_test().query_row("SELECT COUNT(*) FROM ledger_facts WHERE kind='turn.input' AND payload_json LIKE '%delegation_result%'",[],|row|row.get(0)).unwrap();
        assert_eq!(result_inputs, i64::from(index >= 5), "{checkpoint}");
        if (2..5).contains(&index) {
            let state = garive_runtime::reconstruct_suspended_turn(
                &ledger
                    .load_turn(&TurnId::try_from("parent-turn").unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                state.delegation.unwrap().observed,
                index >= 4,
                "{checkpoint}"
            );
        }
    }
}

#[test]
fn killed_scheduler_preserves_each_dispatch_boundary() {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for checkpoint in SCHEDULE_CHECKPOINTS {
        let directory = tempdir().unwrap();
        let database = directory.path().join("scheduler-kill.sqlite3");
        let mut child = Command::new(env!("CARGO_BIN_EXE_garive-runtime-crash-fixture"))
            .args([
                database.to_str().unwrap(),
                repository.to_str().unwrap(),
                checkpoint,
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "READY", "{checkpoint}");
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success(), "{checkpoint}");

        let ledger = SqliteLedger::open(&database).unwrap();
        let session = SessionId::try_from("session").unwrap();
        let state = reconstruct_schedule_state(&ledger, &session, "schedule-1").unwrap();
        assert_eq!(
            state.pending_claim.is_some(),
            *checkpoint != "scheduler_before_claim",
            "{checkpoint}"
        );
        let turns: i64 = ledger
            .connection_for_test()
            .query_row(
                "SELECT COUNT(*) FROM ledger_facts WHERE kind='turn.started'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            turns,
            i64::from(*checkpoint == "scheduler_after_dispatch"),
            "{checkpoint}"
        );
    }
}

#[test]
fn killed_process_recovers_every_durable_checkpoint_without_guessing() {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for checkpoint in CHECKPOINTS {
        let directory = tempdir().unwrap();
        let database = directory.path().join("kill.sqlite3");
        let mut child = Command::new(env!("CARGO_BIN_EXE_garive-runtime-crash-fixture"))
            .args([
                database.to_str().unwrap(),
                repository.to_str().unwrap(),
                checkpoint,
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "READY", "{checkpoint}");
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success(), "{checkpoint}");

        let ledger = SqliteLedger::open(&database).unwrap();
        let session = SessionId::try_from("session").unwrap();
        if *checkpoint == "before_start" {
            assert_eq!(ledger.session_version(&session).unwrap(), Some(1));
            continue;
        }
        let turn_text: String = ledger
            .connection_for_test()
            .query_row(
                "SELECT turn_id FROM ledger_facts WHERE kind = 'turn.started'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let snapshot = ledger
            .load_turn(&TurnId::try_from(turn_text.as_str()).unwrap())
            .unwrap();
        let kinds: Vec<_> = snapshot
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect();
        assert_eq!(
            &kinds[..3],
            ["turn.started", "turn.input", "execution.started"]
        );
        match *checkpoint {
            "after_start" => assert_eq!(kinds.len(), 3),
            "iteration_started" => assert_eq!(kinds.last(), Some(&"execution.iteration_started")),
            "cancel_requested" => assert_eq!(kinds.last(), Some(&"turn.cancel_requested")),
            "model_prepared" => assert_eq!(kinds.last(), Some(&"model.prepared")),
            "model_started" => {
                assert_eq!(kinds.last(), Some(&"model.started"));
                assert_eq!(
                    ledger
                        .list_uncertain_model_requests(&session)
                        .unwrap()
                        .iter()
                        .map(|value| value.as_str())
                        .collect::<Vec<_>>(),
                    ["request"]
                );
            }
            "model_completed" => assert_eq!(kinds.last(), Some(&"model.completed")),
            "effect_prepared" => assert_eq!(kinds.last(), Some(&"effect.prepared")),
            "effect_authorized" => assert_eq!(kinds.last(), Some(&"effect.authorized")),
            "effect_started" => {
                assert_eq!(kinds.last(), Some(&"effect.started"));
                assert_eq!(
                    ledger
                        .list_uncertain_tool_invocations(&session)
                        .unwrap()
                        .iter()
                        .map(|value| value.as_str())
                        .collect::<Vec<_>>(),
                    ["tool"]
                );
            }
            "effect_receipt" => {
                assert_eq!(kinds.last(), Some(&"effect.receipt"));
                assert!(ledger
                    .list_uncertain_tool_invocations(&session)
                    .unwrap()
                    .is_empty());
            }
            "effect_completed" => assert_eq!(kinds.last(), Some(&"effect.completed")),
            "effect_observation" => {
                assert_eq!(kinds.last(), Some(&"effect.observation"));
                assert_eq!(
                    kinds
                        .iter()
                        .filter(|kind| **kind == "effect.observation")
                        .count(),
                    1
                );
            }
            "interaction_requested" => {
                assert_eq!(kinds.last(), Some(&"turn.suspended"));
                assert!(kinds.contains(&"interaction.requested"));
            }
            "interaction_resolved" => {
                assert_eq!(kinds.last(), Some(&"execution.started"));
                assert_eq!(
                    kinds
                        .iter()
                        .filter(|kind| **kind == "interaction.resolved")
                        .count(),
                    1
                );
            }
            "before_terminal" => assert_eq!(kinds.last(), Some(&"effect.observation")),
            "after_terminal" => assert_eq!(
                &kinds[kinds.len() - 2..],
                ["execution.completed", "turn.completed"]
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn killed_owner_cannot_be_replaced_before_lease_recovery() {
    let (_directory, mut ledger, _session, snapshot) = killed_snapshot("after_start");
    let turn = snapshot.facts[0].turn_id.clone().unwrap();
    let execution = snapshot.facts[2].execution_id.clone().unwrap();
    let request = ExecutionLeaseRequest {
        turn_id: turn,
        execution_id: execution,
        owner_id: "replacement".into(),
        lease_token: "replacement-token".into(),
        now_ms: 110,
        duration_ms: 10,
    };
    assert_eq!(
        ledger.acquire_execution_lease(&request),
        Err(ExecutionLeaseError::RecoveryRequired)
    );
}

#[test]
fn recovery_actions_append_their_classification_to_killed_process_state() {
    for (checkpoint, action, terminal) in [
        (
            "model_started",
            RuntimeRecoveryAction::ClassifyModelUncertain,
            "turn.suspended",
        ),
        (
            "effect_started",
            RuntimeRecoveryAction::ClassifyEffectUncertain,
            "turn.suspended",
        ),
        (
            "effect_receipt",
            RuntimeRecoveryAction::RecoverReceiptTerminal,
            "effect.completed",
        ),
    ] {
        let (_directory, mut ledger, session, snapshot) = killed_snapshot(checkpoint);
        let derived = derive_runtime_recovery(&snapshot, 3).unwrap();
        assert_eq!(select_runtime_recovery(derived), action);
        let facts = plan_recovery_action_facts(&snapshot, action, "2026-08-29T00:00:10Z").unwrap();
        assert_eq!(facts.last().unwrap().kind.as_str(), terminal);
        if checkpoint == "model_started" || checkpoint == "effect_started" {
            assert_eq!(facts.len(), 3);
            assert!(facts[0].kind.as_str().ends_with(".uncertain"));
        }
        ledger
            .commit(session, snapshot.session_version, facts)
            .unwrap();
        let recovered = ledger
            .load_turn(&snapshot.facts[0].turn_id.clone().unwrap())
            .unwrap();
        assert_eq!(recovered.facts.last().unwrap().kind.as_str(), terminal);
    }

    let (_directory, mut ledger, session, snapshot) = killed_snapshot("after_start");
    let facts = plan_recovery_action_facts(
        &snapshot,
        RuntimeRecoveryAction::FailRecoveryBound,
        "2026-08-29T00:00:10Z",
    )
    .unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        ["execution.failed", "turn.failed"]
    );
    ledger
        .commit(session, snapshot.session_version, facts)
        .unwrap();
}

fn killed_snapshot(checkpoint: &str) -> (TempDir, SqliteLedger, SessionId, TurnSnapshot) {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = tempdir().unwrap();
    let database = directory.path().join("recovery.sqlite3");
    let mut child = Command::new(env!("CARGO_BIN_EXE_garive-runtime-crash-fixture"))
        .args([
            database.to_str().unwrap(),
            repository.to_str().unwrap(),
            checkpoint,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from("session").unwrap();
    let turn_text: String = ledger
        .connection_for_test()
        .query_row(
            "SELECT turn_id FROM ledger_facts WHERE kind = 'turn.started'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let snapshot = ledger
        .load_turn(&TurnId::try_from(turn_text.as_str()).unwrap())
        .unwrap();
    (directory, ledger, session, snapshot)
}
