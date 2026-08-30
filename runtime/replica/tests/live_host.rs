use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use futures::StreamExt;
use garive_core::{
    AgentOutcome, ExecutionReport, GovernedSuspensionBinding, SuspensionReason, UsageSummary,
};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId, ToolInvocationId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    HostContinuationInput, InstalledAgent, LiveHost, LiveHostError, LiveHostLimits, LiveHostServer,
    SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::oneshot;

const NOW: &str = "2026-08-29T00:00:00Z";

struct FixedClock;

impl HostClock for FixedClock {
    fn recorded_at(&self) -> String {
        NOW.to_owned()
    }
}

struct VerifyingDispatcher {
    database: PathBuf,
    committed: Mutex<Vec<CommittedTurn>>,
}

impl TurnDispatcher for VerifyingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let ledger = SqliteLedger::open(&self.database).unwrap();
        let watermark = ledger.session_watermark(&turn.session_id).unwrap().unwrap();
        assert!(watermark.max_position >= turn.committed_position);
        self.committed.lock().unwrap().push(turn.clone());
        Ok(())
    }
}

struct Harness {
    _directory: TempDir,
    database: PathBuf,
    dispatcher: Arc<VerifyingDispatcher>,
    host: LiveHost,
}

impl Harness {
    fn new(event_batch_size: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("host.sqlite3");
        let dispatcher = Arc::new(VerifyingDispatcher {
            database: database.clone(),
            committed: Mutex::new(Vec::new()),
        });
        let host = LiveHost::new(
            &database,
            installed(),
            LiveHostLimits {
                max_command_bytes: 4_096,
                event_batch_size,
                event_poll_interval_ms: 10,
            },
            Arc::new(FixedClock),
            dispatcher.clone(),
        )
        .unwrap();
        Self {
            _directory: directory,
            database,
            dispatcher,
            host,
        }
    }
}

fn installed() -> InstalledAgent {
    InstalledAgent {
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        agent_instance_namespace: "installed-main".into(),
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(1_024),
            max_output_tokens: Some(512),
            deadline_budget_ms: Some(30_000),
        },
    }
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/host/live-host-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn commands_are_durable_idempotent_and_dispatched_only_after_commit() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    assert_eq!(session.committed_position, 1);
    assert_eq!(
        harness
            .host
            .create_session("create-1", "definition-main")
            .unwrap(),
        session
    );
    assert_eq!(
        harness
            .host
            .start_turn("create-1", &session.session_id, "hello")
            .unwrap_err(),
        LiveHostError::CommandConflict
    );

    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();
    assert_eq!(started.committed_position, 4);
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 1);

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher.clone(),
    )
    .unwrap();
    assert_eq!(
        restarted
            .start_turn("start-1", &session.session_id, "hello")
            .unwrap(),
        started
    );
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 1);
    assert_eq!(
        restarted
            .start_turn("start-1", &session.session_id, "different")
            .unwrap_err(),
        LiveHostError::CommandConflict
    );
    let id = SessionId::try_from(session.session_id.as_str()).unwrap();
    assert_eq!(
        SqliteLedger::open(&harness.database)
            .unwrap()
            .session_watermark(&id)
            .unwrap()
            .unwrap()
            .max_position,
        4
    );
}

#[test]
fn event_projection_advances_over_gaps_and_replays_terminal_text() {
    let harness = Harness::new(1);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();

    let first = harness
        .host
        .read_event_page(&session.session_id, 0)
        .unwrap();
    assert_eq!(first.events[0].event, "session.created");
    let second = harness
        .host
        .read_event_page(&session.session_id, 1)
        .unwrap();
    assert_eq!(second.events[0].event, "turn.started");
    let hidden_input = harness
        .host
        .read_event_page(&session.session_id, 2)
        .unwrap();
    assert!(hidden_input.events.is_empty());
    assert_eq!(hidden_input.scanned_through_position, 3);
    let hidden_execution = harness
        .host
        .read_event_page(&session.session_id, 3)
        .unwrap();
    assert!(hidden_execution.events.is_empty());
    assert_eq!(hidden_execution.scanned_through_position, 4);

    let usage = UsageSummary {
        input_tokens: TokenCount::Known(2),
        output_tokens: TokenCount::Known(1),
        estimated: false,
    };
    let report = ExecutionReport {
        outcome: AgentOutcome::Completed {
            response_items: vec![
                ModelItem::Text { text: "do".into() },
                ModelItem::Refusal { text: "ne".into() },
            ],
            usage,
        },
        completed_iterations: 1,
        usage,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap(),
            execution_id: garive_ledger::ExecutionId::try_from(started.execution_id.as_str())
                .unwrap(),
            recorded_at: NOW.into(),
        },
        &report,
    )
    .unwrap();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(
            SessionId::try_from(session.session_id.as_str()).unwrap(),
            2,
            terminal,
        )
        .unwrap();
    let hidden_terminal = harness
        .host
        .read_event_page(&session.session_id, 4)
        .unwrap();
    assert!(hidden_terminal.events.is_empty());
    let completed = harness
        .host
        .read_event_page(&session.session_id, 5)
        .unwrap();
    assert_eq!(completed.events[0].event, "turn.completed");
    assert_eq!(completed.events[0].text, "done");

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher,
    )
    .unwrap();
    assert_eq!(
        restarted.read_event_page(&session.session_id, 5).unwrap(),
        completed
    );
}

#[test]
fn cancellation_is_a_replayable_request_not_a_terminal_claim() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();
    let cancelled = harness
        .host
        .cancel_turn("cancel-1", &session.session_id, &started.turn_id, 4)
        .unwrap();
    assert_eq!(cancelled.committed_position, 5);
    assert_eq!(
        harness
            .host
            .cancel_turn("cancel-1", &session.session_id, &started.turn_id, 4)
            .unwrap(),
        cancelled
    );
    assert_eq!(
        harness
            .host
            .cancel_turn("cancel-1", &session.session_id, &started.turn_id, 3)
            .unwrap_err(),
        LiveHostError::CommandConflict
    );
}

#[test]
fn continuation_replay_binds_suspension_input_and_expected_version() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        estimated: false,
    };
    let suspended = ExecutionReport {
        outcome: AgentOutcome::Suspended {
            reason: SuspensionReason::PartialOutput,
            partial_items: vec![ModelItem::Text {
                text: "partial".into(),
            }],
            last_durable_position: 4,
            governed_binding: None,
        },
        completed_iterations: 1,
        usage,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap(),
            execution_id: garive_ledger::ExecutionId::try_from(started.execution_id.as_str())
                .unwrap(),
            recorded_at: NOW.into(),
        },
        &suspended,
    )
    .unwrap();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(
            SessionId::try_from(session.session_id.as_str()).unwrap(),
            2,
            terminal,
        )
        .unwrap();
    let ledger = SqliteLedger::open(&harness.database).unwrap();
    let snapshot = ledger
        .load_turn(&garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap())
        .unwrap();
    let state = garive_runtime::reconstruct_suspended_turn(&snapshot).unwrap();
    let continued = harness
        .host
        .continue_turn(
            "continue-1",
            &session.session_id,
            &started.turn_id,
            &state.suspension_id,
            3,
            HostContinuationInput::String("more"),
        )
        .unwrap();
    assert_eq!(continued.committed_position, 9);

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher,
    )
    .unwrap();
    assert_eq!(
        restarted
            .continue_turn(
                "continue-1",
                &session.session_id,
                &started.turn_id,
                &state.suspension_id,
                3,
                HostContinuationInput::String("more"),
            )
            .unwrap(),
        continued
    );
    assert_eq!(
        restarted
            .continue_turn(
                "continue-1",
                &session.session_id,
                &started.turn_id,
                &state.suspension_id,
                4,
                HostContinuationInput::String("more"),
            )
            .unwrap_err(),
        LiveHostError::CommandConflict
    );
}

#[test]
fn interaction_continuation_validates_schema_and_representation_before_commit() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-interaction", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-interaction", &session.session_id, "hello")
        .unwrap();
    let session_id = SessionId::try_from(session.session_id.as_str()).unwrap();
    let turn_id = garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap();
    let execution_id = garive_ledger::ExecutionId::try_from(started.execution_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&harness.database).unwrap();
    ledger
        .commit(
            session_id.clone(),
            2,
            vec![
                FactDraft {
                    fact_id: FactId::try_from("effect-prepared").unwrap(),
                    turn_id: Some(turn_id.clone()),
                    execution_id: Some(execution_id.clone()),
                    model_request_id: None,
                    tool_invocation_id: Some(ToolInvocationId::try_from("tool-1").unwrap()),
                    kind: FactKind::new("effect.prepared").unwrap(),
                    schema_version: 1,
                    payload: CanonicalPayload::from_value(&serde_json::json!({
                        "prepared_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "tool_name":"tool",
                        "tool_revision":"revision",
                        "replay_class":"never_replay",
                        "model_call_id":"call-1"
                    }))
                    .unwrap(),
                    recorded_at: NOW.into(),
                },
                FactDraft {
                fact_id: FactId::try_from("interaction-requested").unwrap(),
                turn_id: Some(turn_id.clone()),
                execution_id: Some(execution_id.clone()),
                model_request_id: None,
                tool_invocation_id: Some(ToolInvocationId::try_from("tool-1").unwrap()),
                kind: FactKind::new("interaction.requested").unwrap(),
                schema_version: 1,
                payload: CanonicalPayload::from_value(&serde_json::json!({
                    "interaction_id":"interaction-1",
                    "suspension_id":"suspension-1",
                    "prepared_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "kind":"approval",
                    "prompt":{"digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","inline_utf8":""},
                    "response_schema":{"digest":"7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553","inline_utf8":"{\"type\":\"boolean\"}"},
                    "response_schema_digest":"7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553",
                    "expiry_code":"none"
                }))
                .unwrap(),
                recorded_at: NOW.into(),
                },
            ],
        )
        .unwrap();
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn_id.clone(),
            execution_id,
            recorded_at: NOW.into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Suspended {
                reason: SuspensionReason::ApprovalRequired,
                partial_items: vec![],
                last_durable_position: 6,
                governed_binding: Some(GovernedSuspensionBinding::Interaction {
                    suspension_id: "suspension-1".into(),
                    interaction_id: "interaction-1".into(),
                    invocation_id: "tool-1".into(),
                    prepared_digest:
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                }),
            },
            completed_iterations: 1,
            usage: UsageSummary {
                input_tokens: TokenCount::Known(1),
                output_tokens: TokenCount::Known(1),
                estimated: false,
            },
        },
    )
    .unwrap();
    ledger.commit(session_id, 3, terminal).unwrap();
    let before = ledger
        .session_watermark(&SessionId::try_from(session.session_id.as_str()).unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(
        harness.host.continue_turn(
            "invalid-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::Json("\"yes\"")
        ),
        Err(LiveHostError::InvalidRequest)
    );
    let after_invalid = ledger
        .session_watermark(&SessionId::try_from(session.session_id.as_str()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(before, after_invalid);
    assert_eq!(
        harness.host.continue_turn(
            "noncanonical-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::Json(" true")
        ),
        Err(LiveHostError::InvalidRequest)
    );

    let continued = harness
        .host
        .continue_turn(
            "continue-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::Json("true"),
        )
        .unwrap();
    assert_eq!(continued.committed_position, 12);
    assert_eq!(
        harness.host.continue_turn(
            "continue-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::String("true")
        ),
        Err(LiveHostError::CommandConflict)
    );
}

#[tokio::test]
async fn real_loopback_http_has_stable_errors_commands_and_sse_replay() {
    let harness = Harness::new(64);
    let server = LiveHostServer::bind(
        harness.host.clone(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
    )
    .await
    .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let client = reqwest::Client::new();
    let base = format!("http://{address}");

    let missing = client
        .post(format!("{base}/v1/sessions"))
        .header("content-type", "application/json")
        .body(r#"{"agent_definition_id":"definition-main"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::BAD_REQUEST);
    let missing: Value = serde_json::from_slice(&missing.bytes().await.unwrap()).unwrap();
    assert_eq!(missing["code"], "invalid_request");

    for (key, body) in [
        (
            "continue-absent",
            r#"{"session_id":"session-x","suspension_id":"suspension-x","expected_session_version":1}"#,
        ),
        (
            "continue-dual",
            r#"{"session_id":"session-x","suspension_id":"suspension-x","expected_session_version":1,"input":"yes","input_json":"true"}"#,
        ),
    ] {
        let response = client
            .post(format!("{base}/v1/turns/turn-x:continue"))
            .header("idempotency-key", key)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    let created = client
        .post(format!("{base}/v1/sessions"))
        .header("idempotency-key", "create-http")
        .header("content-type", "application/json")
        .body(r#"{"agent_definition_id":"definition-main"}"#)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap())
        .unwrap();
    let session_id = created["session_id"].as_str().unwrap();
    let started = client
        .post(format!("{base}/v1/sessions/{session_id}/turns"))
        .header("idempotency-key", "start-http")
        .header("content-type", "application/json")
        .body(r#"{"text":"hello"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(started.status(), reqwest::StatusCode::OK);

    let response = client
        .get(format!(
            "{base}/v1/sessions/{session_id}/events?after_position=0"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut bytes = response.bytes_stream();
    let first = bytes.next().await.unwrap().unwrap();
    let text = String::from_utf8(first.to_vec()).unwrap();
    assert!(text.contains("event: host"));
    assert!(text.contains("session.created"));
    assert!(text.contains(r#""api_version":"v1""#));
    drop(bytes);

    shutdown_tx.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn server_rejects_non_loopback_addresses() {
    let harness = Harness::new(64);
    let result = LiveHostServer::bind(
        harness.host,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
    )
    .await;
    assert!(matches!(
        result,
        Err(garive_runtime::LiveHostServerError::NonLoopbackAddress)
    ));
}

#[test]
fn shared_fixture_enumerates_every_stable_failure_code() {
    let fixture = fixture();
    let expected = fixture["failure_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    let actual = [
        LiveHostError::InvalidRequest,
        LiveHostError::InvalidRequest,
        LiveHostError::NotFound,
        LiveHostError::NotFound,
        LiveHostError::NotFound,
        LiveHostError::CommandConflict,
        LiveHostError::ConcurrentModification,
        LiveHostError::PreconditionFailed,
        LiveHostError::DurabilityUnavailable,
        LiveHostError::CorruptState,
    ]
    .map(LiveHostError::code);
    assert_eq!(expected, actual);
}
