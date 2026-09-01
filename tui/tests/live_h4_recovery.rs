#![cfg(target_os = "macos")]

use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

use garive_core::{
    AgentEvent, AgentEventKind, AgentOutcome, EventSink, ExecutionId, ExecutionReport, SessionId,
    StopReason, TurnId, UsageSummary,
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

static LIVE_H4_TEST_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-09-01T00:00:00Z".into()
    }
}

#[derive(Default)]
struct CancellationResponseGate {
    armed: AtomicBool,
    entered: AtomicBool,
    released: AtomicBool,
    lock: Mutex<()>,
    release: Condvar,
}

impl CancellationResponseGate {
    fn arm(&self) {
        self.entered.store(false, Ordering::SeqCst);
        self.released.store(false, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }

    fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.release.notify_all();
    }
}

impl HostClock for CancellationResponseGate {
    fn recorded_at(&self) -> String {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.entered.store(true, Ordering::SeqCst);
            let mut guard = self.lock.lock().unwrap();
            while !self.released.load(Ordering::SeqCst) {
                let (next, timeout) = self
                    .release
                    .wait_timeout(guard, Duration::from_secs(15))
                    .unwrap();
                guard = next;
                if timeout.timed_out() {
                    break;
                }
            }
        }
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
    let _gate = LIVE_H4_TEST_GATE.lock().await;
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
    let end_log = temporary.path().join("end-follow.log");
    let stage_one = temporary.path().join("stage-one");
    let stage_precommit = temporary.path().join("stage-precommit");
    let stage_two = temporary.path().join("stage-two");
    let stage_three = temporary.path().join("stage-three");
    let stage_four = temporary.path().join("stage-four");
    let state = temporary.path().join("state");
    let reader_log = temporary.path().join("reader.log");
    let reader_ready = temporary.path().join("reader-ready");
    let reader = tokio::task::spawn_blocking({
        let session_id = session.session_id.clone();
        let reader_log = reader_log.clone();
        let reader_ready = reader_ready.clone();
        let reader_state = temporary.path().join("reader-state");
        move || {
            run_screen_reader(
                address,
                &session_id,
                &reader_log,
                &reader_ready,
                &reader_state,
            )
        }
    });
    let expect = tokio::task::spawn_blocking({
        let session_id = session.session_id.clone();
        let end_log = end_log.clone();
        let paths = [
            log.clone(),
            clear_log.clone(),
            recovered_log.clone(),
            final_log.clone(),
            stage_one.clone(),
            stage_precommit.clone(),
            stage_two.clone(),
            stage_three.clone(),
            stage_four.clone(),
            state,
        ];
        move || run_expect(address, &session_id, &paths, &end_log)
    });

    wait_for_with_log(&stage_one, &log).await;
    wait_for(&reader_ready).await;
    assert!(
        host.get_timeline(&session.session_id, 0, 1).unwrap().items[0]
            .completion_text
            .is_none()
    );
    sink.emit(core_event(
        &committed,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: " second-live-frame".into(),
        }),
    ))
    .unwrap();
    wait_for_with_log(&stage_precommit, &log).await;
    let end_follow = fs::read_to_string(&end_log).unwrap();
    assert!(end_follow.contains("before-disconnect"));
    assert!(end_follow.contains("second-live-frame"));
    assert!(!end_follow.contains("End to follow"));
    assert!(
        host.get_timeline(&session.session_id, 0, 1).unwrap().items[0]
            .completion_text
            .is_none()
    );
    proxy_control.send_replace(true);
    wait_for(&stage_two).await;
    let unavailable = fs::read_to_string(&clear_log).unwrap();
    assert!(unavailable.contains("Live"));
    assert!(unavailable.contains("feedback"));
    assert!(unavailable.contains("unavailable"));
    assert!(!unavailable.contains("before-disconnect"));
    assert!(!unavailable.contains("second-live-frame"));

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
    assert_eq!(recovered.matches("second-live-frame").count(), 1);
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
    assert!(reader.await.unwrap());
    let final_screen = fs::read_to_string(&final_log).unwrap();
    assert!(final_screen.contains("durable-authoritative-answer"));
    assert!(!final_screen.contains("Live feedback unavailable"));
    assert!(!final_screen.contains("before-disconnect"));
    assert!(!final_screen.contains("second-live-frame"));
    assert!(!final_screen.contains("after-reconnect"));
    assert!(!final_screen.contains('▍'));
    let reader_output = fs::read_to_string(reader_log).unwrap();
    assert!(!reader_output.contains("before-disconnect"));
    assert!(!reader_output.contains("second-live-frame"));
    assert!(!reader_output.contains("after-reconnect"));
    assert_eq!(
        reader_output
            .matches("durable-authoritative-answer")
            .count(),
        1
    );
    let all = fs::read_to_string(log).unwrap();
    assert!(all.contains("\x1b[?1049h") && all.contains("\x1b[?1049l"));
    assert!(all.contains("\x1b]0;Garive\x07"));
    proxy.abort();
    server_shutdown.send(()).unwrap();
    server_stopped.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shipping_tui_keeps_input_help_and_cancel_responsive_during_a_live_flood() {
    let _gate = LIVE_H4_TEST_GATE.lock().await;
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("runtime.sqlite3");
    let hub = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 1,
        max_preview_bytes: 1_024 * 1_024,
        max_event_bytes: 64,
        broadcast_capacity: 65_536,
        max_subscribers_per_session: 1,
    })
    .unwrap();
    let dispatcher = Arc::new(CaptureDispatcher::default());
    let cancellation_gate = Arc::new(CancellationResponseGate::default());
    let host = LiveHost::new_with_live_output(
        &database,
        installed(),
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 5,
            activity: None,
        },
        cancellation_gate.clone(),
        dispatcher.clone(),
        hub.clone(),
    )
    .unwrap();
    let session = host
        .create_session("create-live-fairness", "definition-main")
        .unwrap();
    let started = host
        .start_turn(
            "start-live-fairness",
            &session.session_id,
            "keep input responsive",
        )
        .unwrap();
    let committed = dispatcher.0.lock().unwrap().clone().unwrap();
    cancellation_gate.arm();
    let mut sink = hub.event_sink();
    sink.emit(core_event(&committed, AgentEventKind::ExecutionStarted))
        .unwrap();
    sink.emit(core_event(
        &committed,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "FLOOD-BEGIN ".into(),
        }),
    ))
    .unwrap();

    let server = LiveHostServer::bind(host.clone(), "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let address = server.local_addr();
    let (server_shutdown, server_stopped) = serve(server);
    let ready = temporary.path().join("ready");
    let active = temporary.path().join("active");
    let cancelled = temporary.path().join("cancelled");
    let draft_seen = temporary.path().join("draft-seen");
    let redraw_seen = temporary.path().join("redraw-seen");
    let terminal_committed = temporary.path().join("terminal-committed");
    let help_seen = temporary.path().join("help-seen");
    let requesting_seen = temporary.path().join("requesting-seen");
    let host_accepted = temporary.path().join("host-accepted");
    let accepted_seen = temporary.path().join("accepted-seen");
    let log = temporary.path().join("fairness.log");
    let state = temporary.path().join("state");
    let expect = tokio::task::spawn_blocking({
        let session_id = session.session_id.clone();
        let paths = [
            log.clone(),
            ready.clone(),
            cancelled.clone(),
            state,
            draft_seen.clone(),
            help_seen.clone(),
            active.clone(),
            redraw_seen.clone(),
            terminal_committed.clone(),
            requesting_seen.clone(),
            host_accepted.clone(),
            accepted_seen.clone(),
        ];
        move || run_fairness_expect(address, &session_id, &paths)
    });

    wait_for(&ready).await;
    sink.emit(core_event(
        &committed,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "FLOOD-ACTIVE ".into(),
        }),
    ))
    .unwrap();
    wait_for(&active).await;
    let stop = Arc::new(AtomicBool::new(false));
    let emitted = Arc::new(AtomicUsize::new(0));
    let flood_hub = hub.clone();
    let flood = tokio::spawn({
        let stop = Arc::clone(&stop);
        let emitted = Arc::clone(&emitted);
        let committed = committed.clone();
        async move {
            let mut flood_sink = flood_hub.event_sink();
            while !stop.load(Ordering::Relaxed) {
                for _ in 0..64 {
                    if flood_sink
                        .emit(core_event(
                            &committed,
                            AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
                                output_index: 0,
                                delta: ".".into(),
                            }),
                        ))
                        .is_err()
                    {
                        return;
                    }
                    emitted.fetch_add(1, Ordering::Relaxed);
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    });

    let script_reached_cancel = tokio::time::timeout(Duration::from_secs(10), async {
        while !cancelled.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if script_reached_cancel.is_err() {
        stop.store(true, Ordering::Relaxed);
        flood.await.unwrap();
        let expect_code = expect.await.unwrap();
        panic!(
            "fairness script stopped before cancel: code={expect_code}, active={}, redraw={}, draft={}, help={}",
            active.exists(),
            redraw_seen.exists(),
            draft_seen.exists(),
            help_seen.exists()
        );
    }
    if tokio::time::timeout(Duration::from_secs(10), async {
        while !requesting_seen.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .is_err()
    {
        cancellation_gate.release();
        let transcript = fs::read_to_string(&log).unwrap_or_default();
        panic!(
            "shipping TUI never rendered requesting cancellation; gate_entered={}, transcript={transcript:?}",
            cancellation_gate.entered.load(Ordering::SeqCst)
        );
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while !cancellation_gate.entered.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cancel request did not reach the gated Host response");
    cancellation_gate.release();
    let cancel_result = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let snapshot = SqliteLedger::open(&database)
                .unwrap()
                .load_turn(&committed.turn_id)
                .unwrap();
            if snapshot
                .facts
                .iter()
                .any(|fact| fact.kind.as_str() == "turn.cancel_requested")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    if cancel_result.is_err() {
        let transcript = fs::read_to_string(&log).unwrap_or_default();
        let diagnostics = fs::read_dir(temporary.path().join("state/diagnostics"))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .collect::<Vec<_>>();
        let pending = fs::read_dir(temporary.path().join("state/pending"))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .collect::<Vec<_>>();
        panic!(
            "shipping cancel did not reach durable Host truth within two seconds; pending={pending:?}, diagnostics={diagnostics:?}, quit_warning={}, help={}, draft={}",
            transcript.contains("Press Ctrl+C again to quit."),
            transcript.contains("Keyboard guide"),
            transcript.contains("draft-under-flood")
        );
    }
    fs::write(&host_accepted, b"ready").unwrap();
    if tokio::time::timeout(Duration::from_secs(25), async {
        while !accepted_seen.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .is_err()
    {
        stop.store(true, Ordering::Relaxed);
        flood.await.unwrap();
        let expect_code = expect.await.unwrap();
        let transcript = fs::read_to_string(&log).unwrap_or_default();
        panic!(
            "fairness script missed accepted cancellation: code={expect_code}, stopping={}, cancelling={}, draft={}",
            transcript.contains("Stopping…"),
            transcript.contains("Cancelling…"),
            transcript.contains("draft-under-flood")
        );
    }
    stop.store(true, Ordering::Relaxed);
    flood.await.unwrap();
    assert!(
        emitted.load(Ordering::Relaxed) > 512,
        "the source exceeded two complete 256-value channel capacities"
    );
    commit_cancelled(&database, &committed);
    hub.end_execution(
        &session.session_id,
        &started.turn_id,
        &started.execution_id,
        LiveOutputEndReason::TerminalCommitted,
    )
    .unwrap();
    fs::write(&terminal_committed, b"ready").unwrap();

    let expect_code = expect.await.unwrap();
    let transcript = fs::read_to_string(log).unwrap();
    if expect_code != 0 {
        let final_frame = transcript.rsplit("\x1b[2J").next().unwrap_or_default();
        let diagnostics = fs::read_dir(temporary.path().join("state/diagnostics"))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .collect::<Vec<_>>();
        panic!(
            "fairness PTY exit code {expect_code}; diagnostics={diagnostics:?}; final_frame={final_frame:?}"
        );
    }
    assert!(transcript.contains("draft-under-flood"));
    assert!(transcript.contains("Keyboard guide"));
    assert!(transcript.contains("Cancelling…"));
    assert!(transcript.contains("Stopping…"));
    assert!(transcript.contains("stopped"));
    assert!(transcript.contains("\x1b[?1049h") && transcript.contains("\x1b[?1049l"));
    server_shutdown.send(()).unwrap();
    server_stopped.await.unwrap().unwrap();
}

fn run_screen_reader(
    address: SocketAddr,
    session: &str,
    log: &Path,
    ready: &Path,
    state: &Path,
) -> bool {
    Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_SESSION", session)
        .env("GARIVE_READER_LOG", log)
        .env("GARIVE_READER_READY", ready)
        .env("GARIVE_TUI_STATE", state)
        .args(["-c", SCREEN_READER_SCRIPT])
        .status()
        .unwrap()
        .success()
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

async fn wait_for_with_log(path: &Path, log: &Path) {
    let waited = tokio::time::timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if waited.is_err() {
        let transcript = fs::read(log).unwrap_or_default();
        let tail = &transcript[transcript.len().saturating_sub(8_192)..];
        panic!(
            "timed out waiting for {}; terminal tail={:?}",
            path.display(),
            String::from_utf8_lossy(tail)
        );
    }
}

fn run_expect(address: SocketAddr, session: &str, paths: &[PathBuf; 10], end_log: &Path) -> bool {
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_SESSION", session)
        .env("GARIVE_ALL_LOG", &paths[0])
        .env("GARIVE_CLEAR_LOG", &paths[1])
        .env("GARIVE_RECOVERED_LOG", &paths[2])
        .env("GARIVE_FINAL_LOG", &paths[3])
        .env("GARIVE_END_LOG", end_log)
        .env("GARIVE_STAGE_ONE", &paths[4])
        .env("GARIVE_STAGE_PRECOMMIT", &paths[5])
        .env("GARIVE_STAGE_TWO", &paths[6])
        .env("GARIVE_STAGE_THREE", &paths[7])
        .env("GARIVE_STAGE_FOUR", &paths[8])
        .env("GARIVE_TUI_STATE", &paths[9])
        .args(["-c", EXPECT_SCRIPT])
        .status()
        .unwrap();
    if !status.success() {
        eprintln!("recovery expect script exited with {status}");
    }
    status.success()
}

fn run_fairness_expect(address: SocketAddr, session: &str, paths: &[PathBuf; 12]) -> i32 {
    Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_SESSION", session)
        .env("GARIVE_FAIRNESS_LOG", &paths[0])
        .env("GARIVE_FAIRNESS_READY", &paths[1])
        .env("GARIVE_FAIRNESS_CANCELLED", &paths[2])
        .env("GARIVE_TUI_STATE", &paths[3])
        .env("GARIVE_FAIRNESS_DRAFT", &paths[4])
        .env("GARIVE_FAIRNESS_HELP", &paths[5])
        .env("GARIVE_FAIRNESS_ACTIVE", &paths[6])
        .env("GARIVE_FAIRNESS_REDRAW", &paths[7])
        .env("GARIVE_FAIRNESS_TERMINAL", &paths[8])
        .env("GARIVE_FAIRNESS_REQUESTING", &paths[9])
        .env("GARIVE_FAIRNESS_HOST_ACCEPTED", &paths[10])
        .env("GARIVE_FAIRNESS_ACCEPTED", &paths[11])
        .args(["-c", FAIRNESS_EXPECT_SCRIPT])
        .status()
        .unwrap()
        .code()
        .unwrap_or(-1)
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
    expect -exact "\033\[2J"
    must "before-disconnect" 21
    send "\033\[Z"
    must "\033\[?25l" 22
    send "\033\[H"
    must "Browsing" 24
    must "history" 26
    mark $env(GARIVE_STAGE_ONE)
    must "1 newer" 23
    log_file
    log_file -a -noappend $env(GARIVE_END_LOG)
    send "\033\[F"
    send "\014"
    must "\033\[2J" 25
    must "before-disconnect" 27
    must "second-live-frame" 29
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_PRECOMMIT)
    must "Live" 31
    must "feedback" 33
    must "unavailable" 35
    log_file
    log_file -a -noappend $env(GARIVE_CLEAR_LOG)
    send "\014"
    must "\033\[2J" 37
    must "Live" 39
    must "feedback" 41
    must "unavailable" 43
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_TWO)
    must "before-disconnect" 45
    must "second-live-frame" 47
    must "after-r" 49
    must "connect" 50
    log_file
    log_file -a -noappend $env(GARIVE_RECOVERED_LOG)
    send "\014"
    must "\033\[2J" 51
    must "before-disconnect" 53
    must "second-live-frame" 55
    must "after-reconnect" 57
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_THREE)
    must "durable-authoritative-answer" 59
    log_file
    log_file -a -noappend $env(GARIVE_FINAL_LOG)
    send "\014"
    must "\033\[2J" 61
    must "durable-authoritative-answer" 63
    log_file
    log_file -a $env(GARIVE_ALL_LOG)
    mark $env(GARIVE_STAGE_FOUR)
    send "\021"
    send "\r"
    expect eof
"#;

const SCREEN_READER_SCRIPT: &str = r#"
    set timeout 10
    encoding system utf-8
    log_user 0
    log_file -a -noappend $env(GARIVE_READER_LOG)
    spawn -noecho /bin/sh -c {exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --session "$GARIVE_TUI_SESSION" --state-dir "$GARIVE_TUI_STATE" --screen-reader}
    expect "Connection online"
    expect "You: recover this stream"
    set f [open $env(GARIVE_READER_READY) w]
    puts $f ready
    close $f
    expect "Garive: durable-authoritative-answer"
    send "\021"
    send "\r"
    expect "Terminal restored."
    expect eof
"#;

const FAIRNESS_EXPECT_SCRIPT: &str = r#"
    set timeout 10
    encoding system utf-8
    log_user 0
    proc mark {path} { set file [open $path w]; puts $file ready; close $file }
    proc wait_file {path code attempts} {
        for {set attempt 0} {$attempt < $attempts} {incr attempt} {
            if {[file exists $path]} { return }
            after 10
        }
        exit $code
    }
    proc must {pattern code} {
        expect {
            -exact $pattern { return }
            timeout { exit $code }
            eof { exit [expr {$code + 1}] }
        }
    }
    proc must_redrawn {pattern code} {
        set previous_timeout $::timeout
        set ::timeout 1
        for {set attempt 0} {$attempt < 10} {incr attempt} {
            send "\014"
            expect {
                -exact $pattern { set ::timeout $previous_timeout; return }
                timeout {}
                eof { exit [expr {$code + 1}] }
            }
        }
        exit $code
    }
    log_file -a -noappend $env(GARIVE_FAIRNESS_LOG)
    spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --session "$GARIVE_TUI_SESSION" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse off}
    fconfigure $spawn_id -encoding utf-8
    must "FLOOD-BEGIN" 21
    mark $env(GARIVE_FAIRNESS_READY)
    must "FLOOD-ACTIVE" 23
    mark $env(GARIVE_FAIRNESS_ACTIVE)
    after 100
    set timeout 2
    send "draft-under-flood"
    send "\011"
    send "\014"
    must "\033\[2J" 24
    mark $env(GARIVE_FAIRNESS_REDRAW)
    must "draft-under-flood" 25
    mark $env(GARIVE_FAIRNESS_DRAFT)
    send "\020"
    send "keyboard"
    send "\014"
    must "\033\[2J" 27
    must "keyboard" 28
    must "/help" 29
    send "\r"
    send "\014"
    must "\033\[2J" 30
    must "Keyboard guide" 31
    mark $env(GARIVE_FAIRNESS_HELP)
    send "\033"
    after 500
    send "\014"
    must "\033\[2J" 32
    must "interrupt" 33
    send "\033"
    mark $env(GARIVE_FAIRNESS_CANCELLED)
    after 100
    must_redrawn "Cancelling…" 35
    must "draft-under-flood" 36
    mark $env(GARIVE_FAIRNESS_REQUESTING)
    wait_file $env(GARIVE_FAIRNESS_HOST_ACCEPTED) 37 2000
    must_redrawn "Stopping…" 39
    must_redrawn "draft-under-flood" 40
    mark $env(GARIVE_FAIRNESS_ACCEPTED)
    set timeout 10
    wait_file $env(GARIVE_FAIRNESS_TERMINAL) 41 1000
    must_redrawn "stopped" 42
    send "x"
    after 100
    send "\014"
    must "\033\[2J" 43
    must "stopped" 44
    must "draft-under-floodx" 45
    send "\021"
    must "Garive?" 46
    send "\r"
    expect {
        eof { exit 0 }
        timeout { exit 48 }
    }
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

fn commit_cancelled(database: &Path, turn: &CommittedTurn) {
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        estimated: false,
    };
    let report = ExecutionReport {
        outcome: AgentOutcome::Stopped {
            reason: StopReason::Cancelled,
        },
        completed_iterations: 0,
        usage,
    };
    let facts = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn.turn_id.clone(),
            execution_id: turn.execution_id.clone(),
            recorded_at: "2026-09-01T00:00:02Z".into(),
        },
        &report,
    )
    .unwrap();
    let ledger = SqliteLedger::open(database).unwrap();
    let version = ledger
        .session_watermark(&turn.session_id)
        .unwrap()
        .unwrap()
        .session_version;
    drop(ledger);
    SqliteLedger::open(database)
        .unwrap()
        .commit(turn.session_id.clone(), version, facts)
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
