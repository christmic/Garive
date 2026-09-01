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

use garive_core::{
    AgentEvent, AgentEventKind, AgentOutcome, EventSink, ExecutionId as CoreExecutionId,
    ExecutionReport, GovernedSuspensionBinding, SessionId as CoreSessionId, StopReason,
    SuspensionReason, TurnId as CoreTurnId, UsageSummary,
};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId, ToolInvocationId};
use garive_llm::{ModelItem, ModelOutputKind, ModelStreamEvent, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer, LiveOutputEndReason, LiveOutputHub,
    LiveOutputLimits, SqliteLedger, TurnDispatchError, TurnDispatcher,
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
    live_output: LiveOutputHub,
}

impl TurnDispatcher for CompletingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let usage = UsageSummary {
            input_tokens: TokenCount::Known(3),
            output_tokens: TokenCount::Known(4),
            estimated: false,
        };
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            schedule_streaming_completion(
                self.database.clone(),
                turn.clone(),
                usage,
                self.live_output.clone(),
            );
            return Ok(());
        }
        if call == 3 {
            schedule_cancel_terminal(self.database.clone(), turn.clone(), usage);
            return Ok(());
        }
        if call == 4 {
            schedule_delayed_completion(self.database.clone(), turn.clone(), usage);
            return Ok(());
        }
        let (outcome, expected_version) = if call == 1 {
            let (binding, last_durable_position) =
                commit_interaction_request(&self.database, turn)?;
            (
                AgentOutcome::Suspended {
                    reason: SuspensionReason::ApprovalRequired,
                    partial_items: vec![ModelItem::Text {
                        text: "partial answer".into(),
                    }],
                    last_durable_position,
                    governed_binding: Some(binding),
                },
                turn.session_version + 1,
            )
        } else {
            (
                AgentOutcome::Completed {
                    response_items: vec![ModelItem::Text {
                        text: "answer after continuation".into(),
                    }],
                    usage,
                },
                turn.session_version,
            )
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
            .and_then(|mut ledger| ledger.commit(turn.session_id.clone(), expected_version, facts))
            .map_err(|_| TurnDispatchError)?;
        Ok(())
    }
}

fn commit_interaction_request(
    database: &Path,
    turn: &CommittedTurn,
) -> Result<(GovernedSuspensionBinding, u64), TurnDispatchError> {
    const EMPTY_DIGEST: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const BOOLEAN_SCHEMA_DIGEST: &str =
        "7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553";
    const PROMPT: &str =
        "{\"action_label_key\":\"suspension.approval.submit\",\"message_text\":\"Approve this continuation?\",\"schema_version\":1,\"title_key\":\"suspension.approval.title\"}";
    const PROMPT_DIGEST: &str = "e7766b2bf1f9f8a45e9049f64d6118cc6dc9513906a641e1812c2973b7d03895";
    let session_id =
        SessionId::try_from(turn.session_id.as_str()).map_err(|_| TurnDispatchError)?;
    let turn_id =
        garive_ledger::TurnId::try_from(turn.turn_id.as_str()).map_err(|_| TurnDispatchError)?;
    let execution_id = garive_ledger::ExecutionId::try_from(turn.execution_id.as_str())
        .map_err(|_| TurnDispatchError)?;
    let invocation_id = ToolInvocationId::try_from("tool-approval").unwrap();
    let payload = |value| CanonicalPayload::from_value(&value).map_err(|_| TurnDispatchError);
    let facts = vec![
        FactDraft {
            fact_id: FactId::try_from("effect-prepared-approval").unwrap(),
            turn_id: Some(turn_id.clone()),
            execution_id: Some(execution_id.clone()),
            model_request_id: None,
            tool_invocation_id: Some(invocation_id.clone()),
            kind: FactKind::new("effect.prepared").unwrap(),
            schema_version: 1,
            payload: payload(serde_json::json!({
                "prepared_digest": EMPTY_DIGEST,
                "tool_name": "approval_fixture",
                "tool_revision": "v1",
                "replay_class": "never_replay",
                "model_call_id": "approval-call"
            }))?,
            recorded_at: "2026-08-30T00:00:01Z".into(),
        },
        FactDraft {
            fact_id: FactId::try_from("interaction-requested-approval").unwrap(),
            turn_id: Some(turn_id),
            execution_id: Some(execution_id),
            model_request_id: None,
            tool_invocation_id: Some(invocation_id),
            kind: FactKind::new("interaction.requested").unwrap(),
            schema_version: 1,
            payload: payload(serde_json::json!({
                "interaction_id": "interaction-approval",
                "suspension_id": "suspension-approval",
                "prepared_digest": EMPTY_DIGEST,
                "kind": "approval",
                "prompt": {"digest": PROMPT_DIGEST, "inline_utf8": PROMPT},
                "response_schema": {
                    "digest": BOOLEAN_SCHEMA_DIGEST,
                    "inline_utf8": "{\"type\":\"boolean\"}"
                },
                "response_schema_digest": BOOLEAN_SCHEMA_DIGEST,
                "expiry_code": "none"
            }))?,
            recorded_at: "2026-08-30T00:00:01Z".into(),
        },
    ];
    let mut ledger = SqliteLedger::open(database).map_err(|_| TurnDispatchError)?;
    ledger
        .commit(session_id.clone(), turn.session_version, facts)
        .map_err(|_| TurnDispatchError)?;
    let last_durable_position = ledger
        .session_watermark(&session_id)
        .map_err(|_| TurnDispatchError)?
        .ok_or(TurnDispatchError)?
        .max_position;
    Ok((
        GovernedSuspensionBinding::Interaction {
            suspension_id: "suspension-approval".into(),
            interaction_id: "interaction-approval".into(),
            invocation_id: "tool-approval".into(),
            prepared_digest: EMPTY_DIGEST.into(),
        },
        last_durable_position,
    ))
}

fn schedule_streaming_completion(
    database: PathBuf,
    turn: CommittedTurn,
    usage: UsageSummary,
    live_output: LiveOutputHub,
) {
    thread::spawn(move || {
        let core_event = |kind| AgentEvent {
            session_id: CoreSessionId::try_from(turn.session_id.as_str()).unwrap(),
            turn_id: CoreTurnId::try_from(turn.turn_id.as_str()).unwrap(),
            execution_id: CoreExecutionId::try_from(turn.execution_id.as_str()).unwrap(),
            kind,
        };
        let mut sink = live_output.event_sink();
        sink.emit(core_event(AgentEventKind::ExecutionStarted))
            .unwrap();
        sink.emit(core_event(AgentEventKind::ModelStream(
            ModelStreamEvent::OutputItemStarted {
                output_index: 0,
                kind: ModelOutputKind::Text,
            },
        )))
        .unwrap();
        sink.emit(core_event(AgentEventKind::ModelStream(
            ModelStreamEvent::TextDelta {
                output_index: 0,
                delta: "first-live-fragment".into(),
            },
        )))
        .unwrap();
        thread::sleep(Duration::from_secs(3));
        sink.emit(core_event(AgentEventKind::ModelStream(
            ModelStreamEvent::TextDelta {
                output_index: 0,
                delta: " final-fragment".into(),
            },
        )))
        .unwrap();
        thread::sleep(Duration::from_millis(180));
        sink.emit(core_event(AgentEventKind::OutcomeProposed))
            .unwrap();
        let report = ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: "first-live-fragment final-fragment".into(),
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
        .unwrap();
        SqliteLedger::open(&database)
            .unwrap()
            .commit(turn.session_id.clone(), turn.session_version, facts)
            .unwrap();
        live_output
            .end_execution(
                turn.session_id.as_str(),
                turn.turn_id.as_str(),
                turn.execution_id.as_str(),
                LiveOutputEndReason::TerminalCommitted,
            )
            .unwrap();
    });
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
        for _ in 0..1_000 {
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
                thread::sleep(Duration::from_millis(500));
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
        let live_output = LiveOutputHub::new(LiveOutputLimits {
            max_active_executions: 4,
            max_preview_bytes: 1_024 * 1_024,
            max_event_bytes: 32 * 1_024,
            broadcast_capacity: 256,
            max_subscribers_per_session: 8,
        })
        .unwrap();
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
            Arc::new(CompletingDispatcher {
                database: database.clone(),
                calls: AtomicUsize::new(0),
                live_output: live_output.clone(),
            }),
            live_output,
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
        let cancel_ready = temporary.path().join("cancel-ready");
        let cancel_stopped = temporary.path().join("cancel-stopped");
        let cancel_monitor = thread::spawn({
            let host = host.clone();
            let database = database.clone();
            let state = state.clone();
            let cancel_ready = cancel_ready.clone();
            let cancel_stopped = cancel_stopped.clone();
            move || {
                // The fixture deliberately holds the first turn open long enough to
                // prove intermediate live frames, then exercises an approval before
                // starting the cancellable turn. Bound the whole scenario, not only
                // the final turn's short cancellation window.
                for _ in 0..1_200 {
                    let sessions = SqliteLedger::open(&database)
                        .unwrap()
                        .list_sessions()
                        .unwrap();
                    let durably_running = sessions.iter().any(|session| {
                        host.get_timeline(session.as_str(), 0, 10)
                            .is_ok_and(|timeline| {
                                timeline.items.iter().any(|item| {
                                    item.user_text == "cancel this turn" && item.state == "running"
                                })
                            })
                    });
                    let pending_is_empty = fs::read_dir(state.join("pending"))
                        .is_ok_and(|mut entries| entries.next().is_none());
                    if durably_running && pending_is_empty {
                        fs::write(&cancel_ready, b"ready").unwrap();
                        break;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                assert!(
                    cancel_ready.exists(),
                    "cancel turn never became durably running"
                );
                for _ in 0..400 {
                    let sessions = SqliteLedger::open(&database)
                        .unwrap()
                        .list_sessions()
                        .unwrap();
                    let stopped = sessions.iter().any(|session| {
                        host.get_timeline(session.as_str(), 0, 10)
                            .is_ok_and(|timeline| {
                                timeline.items.iter().any(|item| {
                                    item.user_text == "cancel this turn" && item.state == "stopped"
                                })
                            })
                    });
                    if stopped {
                        fs::write(&cancel_stopped, b"stopped").unwrap();
                        return;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                panic!("cancel turn never became durably stopped");
            }
        });
        assert!(run_expect(
            address,
            &state,
            &first_log,
            &cancel_stopped,
            false
        ));
        cancel_monitor.join().unwrap();
        let first = fs::read_to_string(&first_log).unwrap();
        assert!(first.as_bytes().contains(&b'\x07'));
        assert!(!first.contains("unavailable"));
        assert!(first.contains("\x1b]0;Garive · Workspace · Connecting · Ready\x07"));
        assert!(first.contains("· Online · Running\x07"));
        assert!(first.contains("\x1b]0;Garive\x07"));
        assert!(first.contains("Agent"));
        assert!(!first.contains("definition-main"));
        assert!(first.contains("retained draftX"));
        assert!(!first.contains("BLOCKED"));

        let durable_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        let sessions = loop {
            let sessions = SqliteLedger::open(&database)
                .unwrap()
                .list_sessions()
                .unwrap();
            let prompts = sessions
                .iter()
                .map(|session| {
                    host.get_timeline(session.as_str(), 0, 10)
                        .unwrap()
                        .items
                        .into_iter()
                        .map(|item| item.user_text)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let foreground_ready = prompts.iter().any(|items| {
                items
                    == &[
                        "hello durable\n耐久 tui".to_owned(),
                        "second question".to_owned(),
                        "cancel this turn".to_owned(),
                    ]
            });
            let background_ready = prompts
                .iter()
                .any(|items| items == &["background task".to_owned()]);
            if sessions.len() == 2 && foreground_ready && background_ready {
                break sessions;
            }
            assert!(
                tokio::time::Instant::now() < durable_deadline,
                "durable scenario did not settle: {prompts:?}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let observed_prompts = sessions
            .iter()
            .map(|session| {
                host.get_timeline(session.as_str(), 0, 10)
                    .unwrap()
                    .items
                    .into_iter()
                    .map(|item| item.user_text)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(sessions.len(), 2, "observed prompts: {observed_prompts:?}");
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
            Some("first-live-fragment final-fragment")
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
        assert!(run_expect(
            address,
            &state,
            &restart_log,
            &cancel_stopped,
            true
        ));
        let restarted = fs::read_to_string(restart_log).unwrap();
        assert!(!restarted.contains("definition-main"));
        assert!(restarted.contains("You: hello durable\n耐久 tui"));
        assert!(restarted.contains("Garive: first-live-fragment final-fragment"));
        assert!(restarted.contains("· Online · Ready\x07"));
        assert!(restarted.contains("\x1b]0;Garive\x07"));
        assert!(SqliteLedger::open(&database)
            .unwrap()
            .session_watermark(&SessionId::try_from(session.as_str()).unwrap())
            .unwrap()
            .is_some());

        let _ = shutdown_tx.send(());
        server_task.await.unwrap().unwrap();
    }
}

fn run_expect(
    address: SocketAddr,
    state: &Path,
    log: &Path,
    cancel_stopped: &Path,
    restart: bool,
) -> bool {
    let script = if restart {
        r#"
            set timeout 8
            encoding system utf-8
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --screen-reader}
            fconfigure $spawn_id -encoding utf-8
            expect "You: hello durable"
            expect "耐久 tui"
            expect "Garive: first-live-fragment final-fragment"
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
            expect "Agent"
            send -- "\033\[200~"
            send -- "hello durable\n\u8010\u4e45 tuX"
            send -- "\033\[201~"
            after 200
            send "\177"
            send "i\r"
            set timeout 2
            expect {
                -exact "first-live-fragment" {}
                timeout { exit 23 }
            }
            set timeout 0
            expect {
                "final-fragment" { exit 24 }
                timeout {}
            }
            set timeout 8
            expect { "first-live-fragment final-fragment" {} timeout { exit 21 } }
            after 500
            send "second question\r"
            expect "Action required"
            send "\r"
            expect "answer after continuation"
            send "cancel this turn\r"
            expect "Online · Running"
            send "retained draft"
            expect "retained draft"
            send "\033"
            expect "Draft locked"
            send "BLOCKED"
            send -- "\033\[<0;5;22M\033\[<32;8;22M\033\[<0;8;22m"
            after 100
            expect "cancel this turn"
            set attempts 0
            while {![file exists $env(GARIVE_CANCEL_STOPPED)]} {
                after 25
                incr attempts
                if {$attempts >= 400} { exit 56 }
            }
            send "\014"
            expect "stopped"
            send "X"
            expect "retained draftX"
            send "\016"
            after 1500
            send "\014"
            expect "0 turns"
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
        .env("GARIVE_CANCEL_STOPPED", cancel_stopped)
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
