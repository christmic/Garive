use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    process::Command,
    sync::{mpsc, Arc},
    thread,
};

use garive_core::{AgentOutcome, ExecutionReport, UsageSummary};
use garive_ledger::{ExecutionId, SessionId, TurnId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer, SqliteLedger, TurnDispatchError,
    TurnDispatcher,
};
use tempfile::TempDir;
use tokio::sync::oneshot;

#[test]
fn tui_renders_ordered_real_h1_events_and_terminal() {
    let server = runtime_host();
    let output = Command::new(env!("CARGO_BIN_EXE_garive-tui"))
        .args([&server.url, "definition-1", "private prompt"])
        .output()
        .expect("TUI must launch");
    server.stop();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let session = stdout
        .find("       1  session.created")
        .expect("Session event");
    let started = stdout.find("       2  turn.started").expect("Turn event");
    let completed = stdout
        .find("       6  turn.completed")
        .expect("terminal event");
    assert!(session < started && started < completed);
    assert!(stdout.contains("Agent: durable answer"));
    assert!(stdout.contains("completed @ position 6"));
}

struct RuntimeHost {
    url: String,
    shutdown: oneshot::Sender<()>,
    task: thread::JoinHandle<()>,
    _directory: TempDir,
}

impl RuntimeHost {
    fn stop(self) {
        self.shutdown.send(()).unwrap();
        self.task.join().unwrap();
    }
}

struct FixedClock;

impl HostClock for FixedClock {
    fn recorded_at(&self) -> String {
        "2026-08-30T00:00:00Z".into()
    }
}

struct CompletingDispatcher {
    database: PathBuf,
}

impl TurnDispatcher for CompletingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let usage = || UsageSummary {
            input_tokens: TokenCount::Known(1),
            output_tokens: TokenCount::Known(2),
            estimated: false,
        };
        let facts = plan_core_terminal(
            &CoreTerminalContext {
                turn_id: TurnId::try_from(turn.turn_id.as_str()).unwrap(),
                execution_id: ExecutionId::try_from(turn.execution_id.as_str()).unwrap(),
                recorded_at: "2026-08-30T00:00:01Z".into(),
            },
            &ExecutionReport {
                outcome: AgentOutcome::Completed {
                    response_items: vec![ModelItem::Text {
                        text: "durable answer".into(),
                    }],
                    usage: usage(),
                },
                completed_iterations: 1,
                usage: usage(),
            },
        )
        .unwrap();
        SqliteLedger::open(&self.database)
            .unwrap()
            .commit(
                SessionId::try_from(turn.session_id.as_str()).unwrap(),
                turn.session_version,
                facts,
            )
            .unwrap();
        Ok(())
    }
}

fn runtime_host() -> RuntimeHost {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("host.sqlite3");
    let (ready_tx, ready_rx) = mpsc::channel();
    let task = thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let host = LiveHost::new(
                    &database,
                    installed(),
                    LiveHostLimits {
                        max_command_bytes: 4096,
                        event_batch_size: 64,
                        event_poll_interval_ms: 5,
                        activity: None,
                    },
                    Arc::new(FixedClock),
                    Arc::new(CompletingDispatcher {
                        database: database.clone(),
                    }),
                )
                .unwrap();
                let server =
                    LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                        .await
                        .unwrap();
                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                ready_tx
                    .send((format!("http://{}/", server.local_addr()), shutdown_tx))
                    .unwrap();
                server
                    .serve(async move {
                        let _ = shutdown_rx.await;
                    })
                    .await
                    .unwrap();
            });
    });
    let (url, shutdown) = ready_rx.recv().unwrap();
    RuntimeHost {
        url,
        shutdown,
        task,
        _directory: directory,
    }
}

fn installed() -> InstalledAgent {
    InstalledAgent {
        definition_id: "definition-1".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        agent_instance_namespace: "installed-main".into(),
        public_capabilities: Vec::new(),
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        public_activity_catalogue: None,
    }
}
