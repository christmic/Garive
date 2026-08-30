use std::sync::Arc;

use garive_host_client::{
    reduce_host_events, ClientLimits, HostActivity, HostClientErrorCode, HostEvent, HostTerminal,
    HostView, LiveHostClient, HOST_CLIENT_FAILURES,
};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

#[derive(Deserialize)]
struct Fixture {
    limits: FixtureLimits,
    session_id: String,
    valid_stream: Vec<HostEvent>,
    expected: Expected,
    reconnect: Reconnect,
    disconnect_before_terminal: Vec<HostEvent>,
    invalid_streams: Vec<InvalidStream>,
    host_errors: Vec<HostError>,
    failure_codes: Vec<String>,
}

#[derive(Deserialize)]
struct FixtureLimits {
    max_command_bytes: usize,
    max_event_bytes: usize,
    max_events: usize,
}

#[derive(Deserialize)]
struct Expected {
    cursor: u64,
    terminal: String,
    text: String,
    unknown_events: Vec<String>,
}

#[derive(Deserialize)]
struct Reconnect {
    after_position: u64,
    events: Vec<HostEvent>,
    expected_applied_positions: Vec<u64>,
}

#[derive(Deserialize)]
struct InvalidStream {
    mutation: String,
    expected: String,
}

#[derive(Deserialize)]
struct HostError {
    status: u16,
    code: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/host/live-host-client-v1.json"
    ))
    .expect("fixture must decode")
}

fn limits(fixture: &Fixture) -> ClientLimits {
    ClientLimits {
        max_command_bytes: fixture.limits.max_command_bytes,
        max_event_bytes: fixture.limits.max_event_bytes,
        max_events: fixture.limits.max_events,
        follow_deadline_ms: 2_000,
    }
}

#[test]
fn shared_fixture_reduces_gaps_unknown_events_and_reconnects() {
    let fixture = fixture();
    let view = reduce_host_events(
        &fixture.session_id,
        &fixture.valid_stream,
        HostView::default(),
        fixture.limits.max_events,
    )
    .expect("valid stream");
    assert_eq!(view.cursor, fixture.expected.cursor);
    assert_eq!(view.terminal, Some(HostTerminal::Completed));
    assert_eq!(fixture.expected.terminal, "completed");
    assert_eq!(view.text, fixture.expected.text);
    assert_eq!(view.unknown_events, fixture.expected.unknown_events);

    let reconnect = reduce_host_events(
        &fixture.session_id,
        &fixture.reconnect.events,
        HostView::at_cursor(fixture.reconnect.after_position),
        fixture.limits.max_events,
    )
    .expect("reconnect stream");
    assert_eq!(
        fixture.reconnect.expected_applied_positions,
        vec![5, reconnect.cursor]
    );

    let disconnected = reduce_host_events(
        &fixture.session_id,
        &fixture.disconnect_before_terminal,
        HostView::default(),
        fixture.limits.max_events,
    )
    .expect("non-terminal prefix remains valid");
    assert_eq!(disconnected.terminal, None);
}

#[test]
fn h3_activity_reducer_preserves_committed_progress_and_rejects_regression() {
    let event = |position: u64, name: &str, state: &str, terminal: bool| HostEvent {
        api_version: "v1".into(),
        session_id: "session-client".into(),
        position,
        event: name.into(),
        turn_id: "turn-client".into(),
        execution_id: "execution-client".into(),
        text: String::new(),
        activity: Some(HostActivity {
            api_version: "v1".into(),
            activity_id: "activity-client".into(),
            kind: "tool".into(),
            label_key: "agent.activity.read".into(),
            state: state.into(),
            source_position: position,
            terminal,
            safe_code: None,
        }),
    };
    let events = [
        event(1, "agent.activity.prepared", "prepared", false),
        event(2, "agent.activity.authorized", "authorized", false),
        event(3, "agent.activity.started", "running", false),
        event(4, "agent.activity.completed", "completed", true),
    ];
    let view = reduce_host_events("session-client", &events, HostView::default(), 10).unwrap();
    assert_eq!(view.activities["activity-client"].state, "completed");
    assert!(view.unknown_events.is_empty());

    let error = reduce_host_events(
        "session-client",
        &[event(5, "agent.activity.started", "running", false)],
        view,
        10,
    )
    .unwrap_err();
    assert_eq!(error.code, HostClientErrorCode::EventOrderViolation);
}

#[test]
fn shared_fixture_mutations_have_exact_failures() {
    let fixture = fixture();
    for case in &fixture.invalid_streams {
        let mut events = fixture.valid_stream.clone();
        let max_events = match case.mutation.as_str() {
            "api_version_v2" => {
                events[0].api_version = "v2".into();
                fixture.limits.max_events
            }
            "session_other" => {
                events[0].session_id = "other".into();
                fixture.limits.max_events
            }
            "position_zero" => {
                events[0].position = 0;
                fixture.limits.max_events
            }
            "position_backward" => {
                events[2].position = 1;
                fixture.limits.max_events
            }
            "duplicate_conflict" => {
                let mut duplicate = events[0].clone();
                duplicate.text = "conflict".into();
                events.insert(1, duplicate);
                fixture.limits.max_events
            }
            "event_count_17" => {
                events = vec![events[0].clone(); 17];
                fixture.limits.max_events
            }
            mutation => panic!("unknown mutation {mutation}"),
        };
        let error = reduce_host_events(
            &fixture.session_id,
            &events,
            HostView::default(),
            max_events,
        )
        .expect_err("mutation must fail");
        assert_eq!(error.code.wire_name(), case.expected);
    }
    assert_eq!(
        HOST_CLIENT_FAILURES
            .iter()
            .map(|code| code.wire_name())
            .collect::<Vec<_>>(),
        fixture.failure_codes
    );
}

#[tokio::test]
async fn live_client_round_trips_real_http_and_sse() {
    let fixture = fixture();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let responses = vec![
        http_json(
            201,
            r#"{"session_id":"session-client","agent_instance_id":"agent-1","committed_position":1}"#,
        ),
        http_json(
            202,
            r#"{"session_id":"session-client","turn_id":"turn-client","execution_id":"execution-client","committed_position":2}"#,
        ),
        http_sse(&fixture.valid_stream),
    ];
    let (base_url, server) = serve(responses, Arc::clone(&bodies)).await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    let session = client
        .create_session("create-stable-1", "definition-1")
        .await
        .expect("create");
    let turn = client
        .start_turn("turn-stable-1", &session.session_id, "private command")
        .await
        .expect("start");
    let view = client
        .follow_until_terminal(&session.session_id, 0)
        .await
        .expect("follow");
    server.await.expect("server task");

    assert_eq!(turn.execution_id, "execution-client");
    assert_eq!(view.terminal, Some(HostTerminal::Completed));
    assert_eq!(view.text, "durable answer");
    let requests = bodies.lock().await;
    assert!(requests[0].starts_with("POST /v1/sessions HTTP/1.1\r\n"));
    assert!(requests[0].contains("idempotency-key: create-stable-1"));
    assert!(requests[1].starts_with("POST /v1/sessions/session-client/turns HTTP/1.1\r\n"));
    assert!(requests[2]
        .starts_with("GET /v1/sessions/session-client/events?after_position=0 HTTP/1.1\r\n"));
}

#[tokio::test]
async fn incremental_follow_applies_backpressure_and_reports_eof_as_unknown() {
    let fixture = fixture();
    let (base_url, server) = serve(
        vec![http_sse(&fixture.valid_stream)],
        Arc::new(Mutex::new(Vec::new())),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let task = tokio::spawn(async move {
        client
            .follow_events("session-client", 0, sender)
            .await
            .unwrap_err()
    });
    let mut positions = Vec::new();
    while let Some(event) = receiver.recv().await {
        positions.push(event.position);
    }
    assert_eq!(positions, [1, 2, 5, 9]);
    assert_eq!(
        task.await.unwrap().code,
        HostClientErrorCode::TransportFailure
    );
    server.await.unwrap();
}

#[tokio::test]
async fn h2_reads_use_exact_paths_and_validate_durable_views() {
    let fixture = fixture();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let session = r#"{"api_version":"v1","session_id":"session-client","agent_instance_id":"agent-1","definition_id":"definition-1","definition_revision":"revision-1","opened_at":"2026-08-30T00:00:00Z","latest_position":4,"latest_turn_id":"turn-client","latest_turn_state":"running","turn_count":1}"#;
    let prompt_digest = "a".repeat(64);
    let responses = vec![
        http_json(
            200,
            r#"{"api_version":"v1","definitions":[{"api_version":"v1","definition_id":"definition-1","definition_revision":"revision-1","capabilities":[]}]}"#,
        ),
        http_json(
            200,
            &format!(r#"{{"api_version":"v1","sessions":[{session}]}}"#),
        ),
        http_json(
            200,
            &format!(r#"{{"api_version":"v1","session":{session},"observed_max_position":4}}"#),
        ),
        http_json(
            200,
            &format!(
                r#"{{"api_version":"v1","session_id":"session-client","items":[{{"turn_id":"turn-client","started_position":2,"latest_position":3,"state":"running","user_text":"hello","content_truncated":false}}],"scanned_through_position":4,"observed_max_position":4,"has_more":false,"ignored_future":"{prompt_digest}"}}"#
            ),
        ),
    ];
    let (base_url, server) = serve(responses, Arc::clone(&requests)).await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    assert_eq!(
        client
            .list_agent_definitions()
            .await
            .unwrap()
            .definitions
            .len(),
        1
    );
    assert_eq!(
        client.list_sessions(10, None).await.unwrap().sessions.len(),
        1
    );
    assert_eq!(
        client
            .get_session("session-client")
            .await
            .unwrap()
            .session
            .turn_count,
        1
    );
    let timeline = client.get_timeline("session-client", 0, 10).await.unwrap();
    assert_eq!(timeline.items[0].user_text, "hello");
    server.await.expect("server task");

    let requests = requests.lock().await;
    assert!(requests[0].starts_with("GET /v1/agent-definitions HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("GET /v1/sessions?limit=10 HTTP/1.1\r\n"));
    assert!(requests[2].starts_with("GET /v1/sessions/session-client HTTP/1.1\r\n"));
    assert!(requests[3].starts_with(
        "GET /v1/sessions/session-client/timeline?after_position=0&limit=10 HTTP/1.1\r\n"
    ));
}

#[tokio::test]
async fn host_fixture_errors_are_typed_without_body_disclosure() {
    let fixture = fixture();
    for host_error in &fixture.host_errors {
        let body = serde_json::json!({"code": host_error.code, "secret": "must-not-leak"});
        let responses = vec![http_json(host_error.status, &body.to_string())];
        let (base_url, server) = serve(responses, Arc::new(Mutex::new(Vec::new()))).await;
        let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
        let error = client
            .create_session("stable", "definition")
            .await
            .expect_err("host error");
        server.await.expect("server task");
        assert_eq!(error.code, HostClientErrorCode::HostFailure);
        assert_eq!(error.status, Some(host_error.status));
        assert!(!format!("{error:?} {error}").contains("must-not-leak"));
    }
}

#[tokio::test]
async fn mutation_methods_use_exact_h1_paths_and_bodies() {
    let fixture = fixture();
    let response = r#"{"session_id":"session-client","turn_id":"turn-client","execution_id":"execution-client","committed_position":12}"#;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, response),
            http_json(200, response),
            http_json(200, response),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    client
        .cancel_turn("cancel-stable", "session-client", "turn-client", 9)
        .await
        .expect("cancel");
    client
        .continue_turn(
            "continue-stable",
            "session-client",
            "turn-client",
            "suspension-client",
            4,
            "approved input",
        )
        .await
        .expect("continue");
    client
        .continue_turn_json(
            "continue-json-stable",
            "session-client",
            "turn-client",
            "suspension-client",
            4,
            &serde_json::json!(true),
        )
        .await
        .expect("typed continue");
    server.await.expect("server task");
    let requests = requests.lock().await;
    assert!(requests[0].starts_with("POST /v1/turns/turn-client:cancel HTTP/1.1\r\n"));
    assert!(requests[0].contains("\"requested_through_position\":9"));
    assert!(requests[1].starts_with("POST /v1/turns/turn-client:continue HTTP/1.1\r\n"));
    assert!(requests[1].contains("\"expected_session_version\":4"));
    assert!(requests[1].contains("\"suspension_id\":\"suspension-client\""));
    assert!(requests[2].starts_with("POST /v1/turns/turn-client:continue HTTP/1.1\r\n"));
    assert!(requests[2].contains("\"input_json\":true"));
}

fn http_json(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn http_sse(events: &[HostEvent]) -> Vec<u8> {
    let body = events
        .iter()
        .map(|event| format!("data: {}\n\n", serde_json::to_string(event).unwrap()))
        .collect::<String>();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn serve(
    responses: Vec<Vec<u8>>,
    requests: Arc<Mutex<Vec<String>>>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 16_384];
            let read = socket.read(&mut bytes).await.unwrap();
            requests
                .lock()
                .await
                .push(String::from_utf8_lossy(&bytes[..read]).into_owned());
            socket.write_all(&response).await.unwrap();
        }
    });
    (format!("http://{address}/"), task)
}
