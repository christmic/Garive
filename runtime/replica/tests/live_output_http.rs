use std::{net::SocketAddr, sync::Arc};

use futures::StreamExt;
use garive_core::{AgentEvent, AgentEventKind, EventSink, ExecutionId, SessionId, TurnId};
use garive_llm::ModelStreamEvent;
use garive_runtime::{
    CommittedTurn, EffectiveRuntimeLimits, HostClock, InstalledAgent, LiveHost, LiveHostLimits,
    LiveHostServer, LiveOutputHub, LiveOutputLimits, TurnDispatchError, TurnDispatcher,
};
use tokio::sync::oneshot;

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-08-31T00:00:00Z".into()
    }
}

struct Dispatcher;
impl TurnDispatcher for Dispatcher {
    fn dispatch(&self, _: &CommittedTurn) -> Result<(), TurnDispatchError> {
        Ok(())
    }
}

fn event(session_id: &str, turn_id: &str, execution_id: &str, kind: AgentEventKind) -> AgentEvent {
    AgentEvent {
        session_id: SessionId::try_from(session_id).unwrap(),
        turn_id: TurnId::try_from(turn_id).unwrap(),
        execution_id: ExecutionId::try_from(execution_id).unwrap(),
        kind,
    }
}

#[tokio::test]
async fn live_route_streams_real_ephemeral_events_without_sse_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("host.sqlite3");
    let hub = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 2,
        max_preview_bytes: 1_024,
        max_event_bytes: 64,
        broadcast_capacity: 16,
        max_subscribers_per_session: 2,
    })
    .unwrap();
    let host = LiveHost::new_with_live_output(
        &database,
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
                deadline_budget_ms: Some(1_000),
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        Arc::new(Dispatcher),
        hub.clone(),
    )
    .unwrap();
    let session = host
        .create_session("create-live-http", "definition-main")
        .unwrap();
    let turn = host
        .start_turn("start-live-http", &session.session_id, "hello")
        .unwrap();

    let server = LiveHostServer::bind(host, "127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let mut sink = hub.event_sink();
    sink.emit(event(
        &session.session_id,
        &turn.turn_id,
        &turn.execution_id,
        AgentEventKind::ExecutionStarted,
    ))
    .unwrap();
    sink.emit(event(
        &session.session_id,
        &turn.turn_id,
        &turn.execution_id,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "progressive".into(),
        }),
    ))
    .unwrap();

    let response = reqwest::Client::new()
        .get(format!(
            "http://{address}/v1/sessions/{}/live",
            session.session_id
        ))
        .header("last-event-id", "999")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let mut body = response.bytes_stream();
    let mut received = String::new();
    while !received.contains(r#""text":"progressive""#) {
        let chunk = body.next().await.unwrap().unwrap();
        received.push_str(std::str::from_utf8(&chunk).unwrap());
    }
    assert!(received.contains("event: live"));
    assert!(!received.lines().any(|line| line.starts_with("id:")));
    assert!(received.contains(r#""api_version":"v1""#));
    assert!(received.contains(&turn.execution_id));

    drop(body);
    shutdown_tx.send(()).unwrap();
    task.await.unwrap().unwrap();
}
