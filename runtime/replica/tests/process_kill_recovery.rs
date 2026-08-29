use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};

use garive_ledger::{SessionId, TurnId};
use garive_runtime::SqliteLedger;
use tempfile::tempdir;

const CHECKPOINTS: &[&str] = &[
    "before_start",
    "after_start",
    "model_prepared",
    "model_started",
    "effect_prepared",
    "effect_authorized",
    "effect_started",
    "effect_receipt",
    "effect_completed",
    "effect_observation",
    "after_terminal",
];

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
            "after_terminal" => assert_eq!(
                &kinds[kinds.len() - 2..],
                ["execution.completed", "turn.completed"]
            ),
            _ => unreachable!(),
        }
    }
}
