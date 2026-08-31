use std::{net::SocketAddr, sync::Arc, time::Duration};

use futures::StreamExt;
use garive_core::{AgentEvent, AgentEventKind, EventSink, ExecutionId, SessionId, TurnId};
use garive_host_client::{ClientLimits, LiveHostClient, LiveOutputEventKind};
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
    let client = LiveHostClient::new(
        format!("http://{address}/").as_str(),
        ClientLimits {
            max_command_bytes: 4_096,
            max_event_bytes: 32_768,
            max_events: 64,
            follow_deadline_ms: 5_000,
        },
    )
    .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let followed_session = session.session_id.clone();
    let follow =
        tokio::spawn(async move { client.follow_live_output(&followed_session, sender).await });
    let snapshot = receiver.recv().await.unwrap();
    assert!(matches!(
        snapshot.kind,
        LiveOutputEventKind::Snapshot {
            ref text,
            through_sequence: 2
        } if text == "progressive"
    ));
    assert_eq!(snapshot.execution_id, turn.execution_id);
    follow.abort();
    shutdown_tx.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn cancelling_live_follow_releases_the_runtime_subscriber_slot() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("subscriber-release.sqlite3");
    let hub = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 1,
        max_preview_bytes: 1_024,
        max_event_bytes: 64,
        broadcast_capacity: 16,
        max_subscribers_per_session: 1,
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
        .create_session("create-subscriber-release", "definition-main")
        .unwrap();
    let turn = host
        .start_turn("start-subscriber-release", &session.session_id, "hello")
        .unwrap();
    let mut sink = hub.event_sink();
    sink.emit(event(
        &session.session_id,
        &turn.turn_id,
        &turn.execution_id,
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "retained preview".into(),
        }),
    ))
    .unwrap();

    let server = LiveHostServer::bind(host, "127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server_task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let client = LiveHostClient::new(
        format!("http://{address}/").as_str(),
        ClientLimits {
            max_command_bytes: 4_096,
            max_event_bytes: 32_768,
            max_events: 64,
            follow_deadline_ms: 5_000,
        },
    )
    .unwrap();

    let (first_sender, mut first_receiver) = tokio::sync::mpsc::channel(1);
    let first_session = session.session_id.clone();
    let first_client = client.clone();
    let first = tokio::spawn(async move {
        first_client
            .follow_live_output(&first_session, first_sender)
            .await
    });
    let first_snapshot = first_receiver.recv().await.unwrap();
    assert!(matches!(
        first_snapshot.kind,
        LiveOutputEventKind::Snapshot { ref text, .. } if text == "retained preview"
    ));
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());

    let mut replacement = None;
    for _ in 0..20 {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let followed_session = session.session_id.clone();
        let followed_client = client.clone();
        let follow = tokio::spawn(async move {
            followed_client
                .follow_live_output(&followed_session, sender)
                .await
        });
        if let Ok(Some(snapshot)) =
            tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await
        {
            replacement = Some((snapshot, follow));
            break;
        }
        follow.abort();
        let _ = follow.await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (snapshot, replacement_follow) = replacement.expect("subscriber slot must be released");
    assert!(matches!(
        snapshot.kind,
        LiveOutputEventKind::Snapshot { ref text, .. } if text == "retained preview"
    ));
    assert_eq!(snapshot.stream_id, first_snapshot.stream_id);
    replacement_follow.abort();
    let _ = replacement_follow.await;

    shutdown_tx.send(()).unwrap();
    server_task.await.unwrap().unwrap();
}
