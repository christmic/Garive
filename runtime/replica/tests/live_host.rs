use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use futures::StreamExt;
use garive_core::{AgentOutcome, ExecutionReport, SuspensionReason, UsageSummary};
use garive_ledger::SessionId;
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    HostReadLimits, InstalledAgent, LiveHost, LiveHostError, LiveHostLimits, LiveHostServer,
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
fn installed_definition_read_is_exact_bounded_and_side_effect_free() {
    let harness = Harness::new(64);
    let before = SqliteLedger::open(&harness.database)
        .unwrap()
        .list_sessions()
        .unwrap();
    let page = harness.host.list_agent_definitions().unwrap();
    assert_eq!(page.api_version, "v1");
    assert_eq!(page.definitions.len(), 1);
    assert_eq!(page.definitions[0].definition_id, "definition-main");
    assert_eq!(page.definitions[0].definition_revision, "revision-1");
    assert!(page.definitions[0].capabilities.is_empty());
    assert_eq!(
        SqliteLedger::open(&harness.database)
            .unwrap()
            .list_sessions()
            .unwrap(),
        before
    );
}

#[test]
fn installed_definition_read_fails_closed_at_response_bound() {
    let harness = Harness::new(64);
    let host = LiveHost::new_with_read_limits(
        &harness.database,
        installed(),
        harness.host.limits(),
        HostReadLimits {
            max_response_bytes: 1,
            ..HostReadLimits::PRODUCT_DEFAULT
        },
        Arc::new(FixedClock),
        harness.dispatcher,
    )
    .unwrap();
    assert_eq!(
        host.list_agent_definitions().unwrap_err(),
        LiveHostError::ReadBoundExceeded
    );
}

#[test]
fn session_view_tracks_first_starts_and_latest_lifecycle() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-view", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-view", &session.session_id, "hello")
        .unwrap();
    let running = harness.host.get_session(&session.session_id).unwrap();
    assert_eq!(running.api_version, "v1");
    assert_eq!(running.session.turn_count, 1);
    assert_eq!(
        running.session.latest_turn_id.as_deref(),
        Some(started.turn_id.as_str())
    );
    assert_eq!(
        running.session.latest_turn_state.as_deref(),
        Some("running")
    );
    assert_eq!(running.observed_max_position, started.committed_position);
    assert_eq!(running.session.opened_at, NOW);
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
    assert_eq!(first.events[0].api_version, "v1");
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
            "more",
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
                "more",
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
                "more",
            )
            .unwrap_err(),
        LiveHostError::CommandConflict
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

    let definitions = client
        .get(format!("{base}/v1/agent-definitions"))
        .send()
        .await
        .unwrap();
    assert_eq!(definitions.status(), reqwest::StatusCode::OK);
    let definitions: Value = serde_json::from_slice(&definitions.bytes().await.unwrap()).unwrap();
    assert_eq!(definitions["api_version"], "v1");
    assert_eq!(
        definitions["definitions"][0]["definition_id"],
        "definition-main"
    );

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
    let session_view = client
        .get(format!("{base}/v1/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(session_view.status(), reqwest::StatusCode::OK);
    let session_view: Value = serde_json::from_slice(&session_view.bytes().await.unwrap()).unwrap();
    assert_eq!(session_view["api_version"], "v1");
    assert_eq!(session_view["session"]["turn_count"], 0);
    assert_eq!(session_view["observed_max_position"], 1);
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
