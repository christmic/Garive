#![cfg(target_os = "macos")]

use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

use garive_core::{
    AgentEvent, AgentEventKind, AgentOutcome, EventSink, ExecutionId, ExecutionReport, SessionId,
    TurnId, UsageSummary,
};
use garive_llm::{ModelItem, ModelStreamEvent, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer, LiveOutputEndReason, LiveOutputHub,
    LiveOutputLimits, SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use tokio::sync::{oneshot, watch};
use tokio::{io::copy_bidirectional, net::TcpListener};

struct Clock;

impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-09-01T00:00:00Z".into()
    }
}

#[derive(Default)]
struct CaptureDispatcher(Mutex<Option<CommittedTurn>>);

impl TurnDispatcher for CaptureDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        *self.0.lock().unwrap() = Some(turn.clone());
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shipping_tui_recovers_live_snapshot_then_converges_to_durable_truth() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("runtime.sqlite3");
    let hub = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 1,
        max_preview_bytes: 1_024,
        max_event_bytes: 64,
        broadcast_capacity: 16,
        max_subscribers_per_session: 2,
    })
    .unwrap();
    let dispatcher = Arc::new(CaptureDispatcher::default());
    let host = LiveHost::new_with_live_output(
        &database,
        installed(),
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        dispatcher.clone(),
        hub.clone(),
    )
    .unwrap();
    let session = host
        .create_session("create-live-recovery", "definition-main")
        .unwrap();
    let started = host
        .start_turn(
            "start-live-recovery",
            &session.session_id,
            "recover this stream",
        )
        .unwrap();
    let committed = dispatcher.0.lock().unwrap().clone().unwrap();
    let mut sink = hub.event_sink();
    sink.emit(core_event(&committed, AgentEventKind::ExecutionStarted))
        .unwrap();
    sink.emit(core_event(
        &committed,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "before-disconnect".into(),
        }),
    ))
    .unwrap();

    let server = LiveHostServer::bind(host.clone(), "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let backend_address = server.local_addr();
    let (server_shutdown, server_stopped) = serve(server);
    let (address, proxy_control, proxy) = proxy(backend_address).await;
    let log = temporary.path().join("all.log");
    let clear_log = temporary.path().join("unavailable.log");
    let recovered_log = temporary.path().join("recovered.log");
    let final_log = temporary.path().join("final.log");
    let stage_one = temporary.path().join("stage-one");
    let stage_two = temporary.path().join("stage-two");
    let stage_three = temporary.path().join("stage-three");
    let stage_four = temporary.path().join("stage-four");
    let state = temporary.path().join("state");
    let expect = tokio::task::spawn_blocking({
        let session_id = session.session_id.clone();
        let paths = [
            log.clone(),
            clear_log.clone(),
            recovered_log.clone(),
            final_log.clone(),
            stage_one.clone(),
            stage_two.clone(),
            stage_three.clone(),
            stage_four.clone(),
            state,
        ];
        move || run_expect(address, &session_id, &paths)
    });

    wait_for(&stage_one).await;
    proxy_control.send_replace(true);
    wait_for(&stage_two).await;
    let unavailable = fs::read_to_string(&clear_log).unwrap();
    assert!(unavailable.contains("Live feedback unavailable"));
    assert!(!unavailable.contains("before-disconnect"));

    sink.emit(core_event(
        &committed,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: " after-reconnect".into(),
        }),
    ))
    .unwrap();
    proxy_control.send_replace(false);
    wait_for(&stage_three).await;
    let recovered = fs::read_to_string(&recovered_log).unwrap();
    assert_eq!(recovered.matches("before-disconnect").count(), 1);
    assert_eq!(recovered.matches("after-reconnect").count(), 1);

    commit_terminal(&database, &committed);
    hub.end_execution(
        &session.session_id,
        &started.turn_id,
        &started.execution_id,
        LiveOutputEndReason::TerminalCommitted,
    )
    .unwrap();
    wait_for(&stage_four).await;
    assert!(expect.await.unwrap());
    let final_screen = fs::read_to_string(&final_log).unwrap();
    assert!(final_screen.contains("durable-authoritative-answer"));
    assert!(!final_screen.contains("Live feedback unavailable"));
    assert!(!final_screen.contains("before-disconnect after-reconnect"));
    assert!(!final_screen.contains('▍'));
    let all = fs::read_to_string(log).unwrap();
    assert!(all.contains("\x1b[?1049h") && all.contains("\x1b[?1049l"));
    assert!(all.contains("\x1b]0;Garive\x07"));
    proxy.abort();
    server_shutdown.send(()).unwrap();
    server_stopped.await.unwrap().unwrap();
}

async fn proxy(
    backend: SocketAddr,
) -> (SocketAddr, watch::Sender<bool>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (control, _) = watch::channel(false);
    let proxy_control = control.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut client, _) = listener.accept().await.unwrap();
            let mut disconnect = control.subscribe();
            if *disconnect.borrow() {
                continue;
            }
            tokio::spawn(async move {
                let mut host = tokio::net::TcpStream::connect(backend).await.unwrap();
                tokio::select! {
                    _ = copy_bidirectional(&mut client, &mut host) => {}
                    _ = disconnect.wait_for(|value| *value) => {}
                }
            });
        }
    });
    (address, proxy_control, task)
}

fn serve(
    server: LiveHostServer,
) -> (
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<(), garive_runtime::LiveHostServerError>>,
) {
    let (shutdown, stopped) = oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = stopped.await;
    }));
    (shutdown, task)
}

async fn wait_for(path: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {}", path.display()));
}

fn run_expect(address: SocketAddr, session: &str, paths: &[PathBuf; 9]) -> bool {
    Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_SESSION", session)
        .env("GARIVE_ALL_LOG", &paths[0])
        .env("GARIVE_CLEAR_LOG", &paths[1])
        .env("GARIVE_RECOVERED_LOG", &paths[2])
        .env("GARIVE_FINAL_LOG", &paths[3])
        .env("GARIVE_STAGE_ONE", &paths[4])
        .env("GARIVE_STAGE_TWO", &paths[5])
        .env("GARIVE_STAGE_THREE", &paths[6])
        .env("GARIVE_STAGE_FOUR", &paths[7])
        .env("GARIVE_TUI_STATE", &paths[8])
        .args(["-c", EXPECT_SCRIPT])
        .status()
        .unwrap()
        .success()
}

const EXPECT_SCRIPT: &str = r#"
    set timeout 10
    encoding system utf-8
    log_user 0
    proc mark {path} { set f [open $path w]; puts $f ready; close $f }
    proc must {pattern code} {
        expect {
            -exact $pattern { return }
            timeout { exit $code }
            eof { exit [expr {$code + 1}] }
        }
    }
    log_file -a -noappend $env(GARIVE_ALL_LOG)
    spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --session "$GARIVE_TUI_SESSION" --state-dir "$GARIVE_TUI_STATE" --theme mono}
    fconfigure $spawn_id -encoding utf-8
    expect -exact "\033\[6n"
    send "\033\[1;1R"
    must "before-disconnect" 21
    mark $env(GARIVE_STAGE_ONE)
    must "Live feedback unavailable" 31
    log_file
    log_file -a -noappend $env(GARIVE_CLEAR_LOG)
    send "\033\[Z"
    send "\014"
    must "\033\[6n" 33
    send "\033\[1;1R"
    must "Live feedback unavailable" 35
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_TWO)
    must "before-disconnect after-reconnect" 41
    log_file
    log_file -a -noappend $env(GARIVE_RECOVERED_LOG)
    send "\014"
    must "\033\[6n" 43
    send "\033\[1;1R"
    must "before-disconnect" 45
    must "after-reconnect" 47
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_THREE)
    must "durable-authoritative-answer" 51
    log_file
    log_file -a -noappend $env(GARIVE_FINAL_LOG)
    send "\014"
    must "\033\[6n" 53
    send "\033\[1;1R"
    must "durable-authoritative-answer" 55
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_FOUR)
    send "\021"
    send "\r"
    expect eof
"#;

fn core_event(turn: &CommittedTurn, kind: AgentEventKind) -> AgentEvent {
    AgentEvent {
        session_id: SessionId::try_from(turn.session_id.as_str()).unwrap(),
        turn_id: TurnId::try_from(turn.turn_id.as_str()).unwrap(),
        execution_id: ExecutionId::try_from(turn.execution_id.as_str()).unwrap(),
        kind,
    }
}

fn commit_terminal(database: &Path, turn: &CommittedTurn) {
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(2),
        estimated: false,
    };
    let report = ExecutionReport {
        outcome: AgentOutcome::Completed {
            response_items: vec![ModelItem::Text {
                text: "durable-authoritative-answer".into(),
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
            recorded_at: "2026-09-01T00:00:01Z".into(),
        },
        &report,
    )
    .unwrap();
    SqliteLedger::open(database)
        .unwrap()
        .commit(turn.session_id.clone(), turn.session_version, facts)
        .unwrap();
}

fn installed() -> InstalledAgent {
    InstalledAgent {
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "a".repeat(64),
        agent_instance_namespace: "local-main".into(),
        public_capabilities: Vec::new(),
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: Some(20),
            max_output_tokens: Some(20),
            deadline_budget_ms: Some(10_000),
        },
        public_activity_catalogue: None,
    }
}
