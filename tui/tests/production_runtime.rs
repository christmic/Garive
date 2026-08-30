use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use garive_core::{AgentOutcome, ExecutionReport, StopReason, SuspensionReason, UsageSummary};
use garive_ledger::SessionId;
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer, SqliteLedger, TurnDispatchError,
    TurnDispatcher,
};

struct Clock;

impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-08-30T00:00:00Z".into()
    }
}

struct CompletingDispatcher {
    database: PathBuf,
    calls: AtomicUsize,
}

impl TurnDispatcher for CompletingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let usage = UsageSummary {
            input_tokens: TokenCount::Known(3),
            output_tokens: TokenCount::Known(4),
            estimated: false,
        };
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 3 {
            schedule_cancel_terminal(self.database.clone(), turn.clone(), usage);
            return Ok(());
        }
        if call == 4 {
            schedule_delayed_completion(self.database.clone(), turn.clone(), usage);
            return Ok(());
        }
        let outcome = if call == 1 {
            AgentOutcome::Suspended {
                reason: SuspensionReason::PartialOutput,
                partial_items: vec![ModelItem::Text {
                    text: "partial answer".into(),
                }],
                last_durable_position: turn.committed_position,
                governed_binding: None,
            }
        } else {
            AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: if call == 0 {
                        "answer from production runtime"
                    } else {
                        "answer after continuation"
                    }
                    .into(),
                }],
                usage,
            }
        };
        let report = ExecutionReport {
            outcome,
            completed_iterations: 1,
            usage,
        };
        let facts = plan_core_terminal(
            &CoreTerminalContext {
                turn_id: turn.turn_id.clone(),
                execution_id: turn.execution_id.clone(),
                recorded_at: "2026-08-30T00:00:01Z".into(),
            },
            &report,
        )
        .map_err(|_| TurnDispatchError)?;
        SqliteLedger::open(&self.database)
            .and_then(|mut ledger| {
                ledger.commit(turn.session_id.clone(), turn.session_version, facts)
            })
            .map_err(|_| TurnDispatchError)?;
        Ok(())
    }
}

fn schedule_delayed_completion(database: PathBuf, turn: CommittedTurn, usage: UsageSummary) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1_200));
        let report = ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: "background completion".into(),
                }],
                usage,
            },
            completed_iterations: 1,
            usage,
        };
        let facts = plan_core_terminal(
            &CoreTerminalContext {
                turn_id: turn.turn_id,
                execution_id: turn.execution_id,
                recorded_at: "2026-08-30T00:00:03Z".into(),
            },
            &report,
        )
        .unwrap();
        SqliteLedger::open(&database)
            .unwrap()
            .commit(turn.session_id, turn.session_version, facts)
            .unwrap();
    });
}

fn schedule_cancel_terminal(database: PathBuf, turn: CommittedTurn, usage: UsageSummary) {
    thread::spawn(move || {
        for _ in 0..200 {
            let ledger = SqliteLedger::open(&database).unwrap();
            let Ok(snapshot) = ledger.load_turn(&turn.turn_id) else {
                thread::sleep(Duration::from_millis(10));
                continue;
            };
            if snapshot
                .facts
                .iter()
                .any(|fact| fact.kind.as_str() == "turn.cancel_requested")
            {
                let version = ledger
                    .session_watermark(&turn.session_id)
                    .unwrap()
                    .unwrap()
                    .session_version;
                drop(ledger);
                let report = ExecutionReport {
                    outcome: AgentOutcome::Stopped {
                        reason: StopReason::Cancelled,
                    },
                    completed_iterations: 0,
                    usage,
                };
                let facts = plan_core_terminal(
                    &CoreTerminalContext {
                        turn_id: turn.turn_id,
                        execution_id: turn.execution_id,
                        recorded_at: "2026-08-30T00:00:02Z".into(),
                    },
                    &report,
                )
                .unwrap();
                SqliteLedger::open(&database)
                    .unwrap()
                    .commit(turn.session_id, version, facts)
                    .unwrap();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("TUI never committed the cancellation request");
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shipping_tui_round_trips_through_production_sqlite_runtime() {
    for _ in 0..2 {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("runtime.sqlite3");
        let host = LiveHost::new(
            &database,
            installed(),
            LiveHostLimits {
                max_command_bytes: 4_096,
                event_batch_size: 64,
                event_poll_interval_ms: 10,
                activity: None,
            },
            Arc::new(Clock),
            Arc::new(CompletingDispatcher {
                database: database.clone(),
                calls: AtomicUsize::new(0),
            }),
        )
        .unwrap();
        let server = LiveHostServer::bind(host.clone(), "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let address = server.local_addr();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server_task = tokio::spawn(server.serve(async move {
            let _ = shutdown_rx.await;
        }));

        let state = temporary.path().join("state");
        let first_log = temporary.path().join("first.log");
        assert!(run_expect(address, &state, &first_log, false));
        let first = fs::read_to_string(&first_log).unwrap();
        assert!(first.contains("answer"));
        assert!(first.contains("production"));
        assert!(first.contains("runtime"));
        assert!(first.as_bytes().contains(&b'\x07'));
        assert!(!first.contains("unavailable"));

        let sessions = SqliteLedger::open(&database)
            .unwrap()
            .list_sessions()
            .unwrap();
        assert_eq!(sessions.len(), 2);
        let session = sessions
            .iter()
            .find(|session| {
                host.get_timeline(session.as_str(), 0, 10)
                    .unwrap()
                    .items
                    .len()
                    == 3
            })
            .unwrap()
            .clone();
        let timeline = host.get_timeline(session.as_str(), 0, 10).unwrap();
        assert_eq!(timeline.items[0].user_text, "hello durable\n耐久 tui");
        assert_eq!(
            timeline.items[0].completion_text.as_deref(),
            Some("answer from production runtime")
        );
        assert_eq!(timeline.items[1].user_text, "second question");
        assert_eq!(
            timeline.items[1].completion_text.as_deref(),
            Some("answer after continuation")
        );
        assert_eq!(timeline.items[2].user_text, "cancel this turn");
        assert_eq!(timeline.items[2].state, "stopped");
        let background = sessions.iter().find(|value| **value != session).unwrap();
        let background_timeline = host.get_timeline(background.as_str(), 0, 10).unwrap();
        assert_eq!(background_timeline.items[0].user_text, "background task");
        assert_eq!(
            background_timeline.items[0].completion_text.as_deref(),
            Some("background completion")
        );
        let preferences: serde_json::Value =
            serde_json::from_slice(&fs::read(state.join("preferences.v1.json")).unwrap()).unwrap();
        assert_eq!(
            preferences["selected_session_id"].as_str(),
            Some(session.as_str())
        );

        let restart_log = temporary.path().join("restart.log");
        assert!(run_expect(address, &state, &restart_log, true));
        let restarted = fs::read_to_string(restart_log).unwrap();
        assert!(restarted.contains("You: hello durable\n耐久 tui"));
        assert!(restarted.contains("Garive: answer from production runtime"));
        assert!(SqliteLedger::open(&database)
            .unwrap()
            .session_watermark(&SessionId::try_from(session.as_str()).unwrap())
            .unwrap()
            .is_some());

        let _ = shutdown_tx.send(());
        server_task.await.unwrap().unwrap();
    }
}

fn run_expect(address: SocketAddr, state: &Path, log: &Path, restart: bool) -> bool {
    let script = if restart {
        r#"
            set timeout 8
            encoding system utf-8
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --screen-reader}
            fconfigure $spawn_id -encoding utf-8
            expect "You: hello durable"
            expect "耐久 tui"
            expect "Garive: answer from production runtime"
            expect "You: second question"
            expect "Garive: answer after continuation"
            expect "You: cancel this turn"
            send "\021"
            send "\r"
            expect eof
        "#
    } else {
        r#"
            set timeout 8
            encoding system utf-8
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono}
            fconfigure $spawn_id -encoding utf-8
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "definition-m"
            send -- "\033\[200~"
            send -- "hello durable\n\u8010\u4e45 tuX"
            send -- "\033\[201~"
            after 200
            send "\177"
            send "i\r"
            send "?"
            expect { "Keyboard guide" {} timeout { exit 20 } }
            after 300
            send "\033"
            expect { "answer from production runtime" {} timeout { exit 21 } }
            send "second question\r"
            expect "Action required"
            send "\r"
            after 300
            send "continue please\r"
            expect "answer after continuation"
            send "cancel this turn\r"
            after 300
            send "\003"
            expect "stopped"
            send "\016"
            after 300
            send "background task\r"
            expect "background task"
            send "\023"
            expect "Switch session"
            send "\t"
            send "\r"
            expect "cancel this turn"
            expect "background Session reached a terminal state"
            send "\021"
            send "\r"
            expect eof
        "#
    };
    Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", log)
        .env("GARIVE_TUI_STATE", state)
        .args(["-c", script])
        .status()
        .unwrap()
        .success()
}

fn installed() -> InstalledAgent {
    InstalledAgent {
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "a".repeat(64),
        agent_instance_namespace: "installed-main".into(),
        public_capabilities: Vec::new(),
        public_activity_catalogue: None,
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(1_024),
            max_output_tokens: Some(512),
            deadline_budget_ms: Some(30_000),
        },
    }
}
