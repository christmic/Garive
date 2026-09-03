use std::{
    fs,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    path::PathBuf,
    process::Command,
    sync::{mpsc, Arc},
    thread,
};

use garive_core::{AgentFailureReason, AgentOutcome, ExecutionReport, UsageSummary};
use garive_ledger::{ExecutionId, SessionId, TurnId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, CreateAgentRequest,
    EffectiveRuntimeLimits, HostClock, InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer,
    SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use tempfile::TempDir;
use tokio::sync::oneshot;

#[test]
fn cli_uses_real_h1_and_prints_committed_completion() {
    let server = runtime_host(TerminalMode::Completed);
    let output = Command::new(env!("CARGO_BIN_EXE_garive"))
        .args([&server.url, "definition-1", "private prompt"])
        .output()
        .expect("CLI must launch");
    server.stop();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "durable answer\n"
    );
}

#[test]
fn cli_maps_durable_failure_to_exit_five() {
    let server = runtime_host(TerminalMode::Failed);
    let status = Command::new(env!("CARGO_BIN_EXE_garive"))
        .args([&server.url, "definition-1", "private prompt"])
        .status()
        .expect("CLI must launch");
    server.stop();
    assert_eq!(status.code(), Some(5));
}

#[test]
fn cli_reuses_an_explicit_session_without_creating_another() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            json_response(
                r#"{"session_id":"session-1","turn_id":"turn-1","execution_id":"execution-1","committed_position":2}"#,
            ),
            sse_response("turn.completed", "reused answer"),
        ];
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 8_192];
            let read = socket.read(&mut request).unwrap();
            assert!(!String::from_utf8_lossy(&request[..read]).contains("POST /v1/sessions HTTP"));
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_garive"))
        .args([
            &format!("http://{address}/"),
            "--session",
            "session-1",
            "again",
        ])
        .output()
        .expect("CLI must launch");
    server.join().expect("Host server must finish");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "reused answer\n");
}

#[derive(Clone, Copy)]
enum TerminalMode {
    Completed,
    Failed,
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
    mode: TerminalMode,
}

impl TurnDispatcher for CompletingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let usage = || UsageSummary {
            input_tokens: TokenCount::Known(1),
            output_tokens: TokenCount::Known(2),
            estimated: false,
        };
        let outcome = match self.mode {
            TerminalMode::Completed => AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: "durable answer".into(),
                }],
                usage: usage(),
            },
            TerminalMode::Failed => AgentOutcome::Failed {
                reason: AgentFailureReason::PortFailure,
            },
        };
        let facts = plan_core_terminal(
            &CoreTerminalContext {
                turn_id: TurnId::try_from(turn.turn_id.as_str()).unwrap(),
                execution_id: ExecutionId::try_from(turn.execution_id.as_str()).unwrap(),
                recorded_at: "2026-08-30T00:00:01Z".into(),
            },
            &ExecutionReport {
                outcome,
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

fn runtime_host(mode: TerminalMode) -> RuntimeHost {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("host.sqlite3");
    let working = directory.path().join("agent");
    fs::create_dir(&working).unwrap();
    fs::write(working.join("AGENT.md"), "# Agent\n").unwrap();
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
                        mode,
                    }),
                )
                .unwrap();
                host.create_agent(
                    "define-cli-agent",
                    &CreateAgentRequest {
                        agent_id: "definition-1".into(),
                        working_directory: working,
                        readonly_knowledge_directories: Vec::new(),
                        writable_knowledge_directory: None,
                    },
                )
                .unwrap();
                host.activate_agent("activate-cli-agent", "definition-1")
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

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn sse_response(terminal: &str, text: &str) -> String {
    let body = format!(
        "data: {{\"api_version\":\"v1\",\"session_id\":\"session-1\",\"position\":3,\"event\":\"{terminal}\",\"turn_id\":\"turn-1\",\"execution_id\":\"execution-1\",\"text\":\"{text}\"}}\n\n"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
