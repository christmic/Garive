#![cfg(feature = "test-hooks")]

use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use garive_core::{AgentOutcome, ExecutionReport, UsageSummary};
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
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let usage = UsageSummary {
            input_tokens: TokenCount::Known(2),
            output_tokens: TokenCount::Known(3),
            estimated: false,
        };
        let report = ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: format!("recovered completion {call}"),
                }],
                usage,
            },
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

#[derive(Clone, Copy)]
struct CrashCase {
    argument: &'static str,
    marker: &'static str,
    pending_after_kill: bool,
    turns_after_kill: usize,
}

const CASES: [CrashCase; 3] = [
    CrashCase {
        argument: "pending-persisted",
        marker: "GARIVE_TEST_CRASH_HOOK=pending-persisted",
        pending_after_kill: true,
        turns_after_kill: 0,
    },
    CrashCase {
        argument: "response-accepted",
        marker: "GARIVE_TEST_CRASH_HOOK=response-accepted",
        pending_after_kill: true,
        turns_after_kill: 1,
    },
    CrashCase {
        argument: "pending-removed",
        marker: "GARIVE_TEST_CRASH_HOOK=pending-removed",
        pending_after_kill: false,
        turns_after_kill: 1,
    },
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shipping_process_recovers_every_pending_command_crash_boundary() {
    for case in CASES {
        run_case(case).await;
    }
}

async fn run_case(case: CrashCase) {
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
    let session = host
        .create_session("seed-session", "definition-main")
        .unwrap()
        .session_id;
    let server = LiveHostServer::bind(host.clone(), "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let state = temporary.path().join("state");

    assert!(run_crash(address, &state, &session, case));
    let pending = pending_files(&state);
    assert_eq!(pending.len(), usize::from(case.pending_after_kill));
    if let Some(path) = pending.first() {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value["request_payload"]["text"], "first crash boundary");
        assert!(value["command_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()));
        assert_eq!(value["request_digest"].as_str().unwrap().len(), 64);
    }
    assert_eq!(
        host.get_timeline(&session, 0, 10).unwrap().items.len(),
        case.turns_after_kill
    );

    assert!(run_restart(
        address,
        &state,
        &session,
        case.pending_after_kill
    ));
    assert!(pending_files(&state).is_empty());
    let timeline = host.get_timeline(&session, 0, 10).unwrap();
    assert_eq!(timeline.items.len(), 2);
    assert_eq!(timeline.items[0].user_text, "first crash boundary");
    assert_eq!(
        timeline.items[0].completion_text.as_deref(),
        Some("recovered completion 1")
    );
    assert_eq!(timeline.items[1].user_text, "second after recovery");
    assert_eq!(
        timeline.items[1].completion_text.as_deref(),
        Some("recovered completion 2")
    );

    let _ = shutdown_tx.send(());
    server_task.await.unwrap().unwrap();
}

fn run_crash(address: SocketAddr, state: &Path, session: &str, case: CrashCase) -> bool {
    Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_STATE", state)
        .env("GARIVE_TUI_SESSION", session)
        .env("GARIVE_TUI_HOOK", case.argument)
        .env("GARIVE_TUI_MARKER", case.marker)
        .args(["-c", CRASH_SCRIPT])
        .status()
        .unwrap()
        .success()
}

fn run_restart(address: SocketAddr, state: &Path, session: &str, recovery: bool) -> bool {
    Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_STATE", state)
        .env("GARIVE_TUI_SESSION", session)
        .env("GARIVE_TUI_RECOVERY", if recovery { "yes" } else { "no" })
        .args(["-c", RESTART_SCRIPT])
        .status()
        .unwrap()
        .success()
}

const CRASH_SCRIPT: &str = r#"
    set timeout 8
    proc must_expect {pattern code} {
        expect {
            -exact $pattern { return }
            timeout { exit $code }
            eof { exit $code }
        }
    }
    spawn -noecho /bin/sh -c {exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --session "$GARIVE_TUI_SESSION" --state-dir "$GARIVE_TUI_STATE" --screen-reader --test-crash-hook "$GARIVE_TUI_HOOK"}
    must_expect "Connection online" 20
    send "first crash boundary\r"
    must_expect $env(GARIVE_TUI_MARKER) 21
    set child $spawn_id
    exec kill -KILL [exp_pid -i $child]
    expect eof
    catch wait result
    if {[lindex $result 4] ne "CHILDKILLED" || [lindex $result 5] ne "SIGKILL"} { exit 22 }
"#;

const RESTART_SCRIPT: &str = r#"
    set timeout 8
    proc must_expect {pattern code} {
        expect {
            -exact $pattern { return }
            timeout { exit $code }
            eof { exit $code }
        }
    }
    spawn -noecho /bin/sh -c {exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --session "$GARIVE_TUI_SESSION" --state-dir "$GARIVE_TUI_STATE" --screen-reader}
    if {$env(GARIVE_TUI_RECOVERY) == "yes"} {
        must_expect "Command result unknown" 30
        send "\r"
    }
    must_expect "You: first crash boundary" 31
    must_expect "Garive: recovered completion 1" 32
    after 500
    send "second after recovery\r"
    must_expect "Garive: recovered completion 2" 33
    send "\021"
    send "\r"
    must_expect "Terminal restored." 34
"#;

fn pending_files(state: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(state.join("pending"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    files.sort();
    files
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
