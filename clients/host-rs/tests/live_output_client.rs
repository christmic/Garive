use std::time::Duration;

use garive_host_client::{
    ClientLimits, HostClientErrorCode, LiveHostClient, LiveOutputEvent, LiveOutputEventKind,
};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

const STREAM_A: &str = "12345678-1234-4234-8234-123456789abc";
const STREAM_B: &str = "22345678-1234-4234-8234-123456789abc";
const SESSION: &str = "session-live";

fn limits(max_events: usize) -> ClientLimits {
    ClientLimits {
        max_command_bytes: 4_096,
        max_event_bytes: 32_768,
        max_events,
        follow_deadline_ms: 2_000,
    }
}

fn wire(stream: &str, sequence: u64, kind: &str) -> Value {
    let mut value = json!({
        "api_version": "v1",
        "session_id": SESSION,
        "turn_id": "turn-live",
        "execution_id": "execution-live",
        "stream_id": stream,
        "sequence": sequence,
        "kind": kind,
    });
    if kind == "text_delta" {
        value["text"] = json!(format!("delta-{sequence}"));
    } else if kind == "ended" {
        value["reason"] = json!("terminal_committed");
    }
    value
}

fn live_body(values: &[Value]) -> String {
    values
        .iter()
        .map(|value| {
            format!(
                "event: live\ndata: {}\n\n",
                serde_json::to_string(value).unwrap()
            )
        })
        .collect()
}

async fn serve_body(body: String) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (format!("http://{address}/"), task)
}

async fn follow(
    body: String,
    capacity: usize,
    max_events: usize,
) -> (
    HostClientErrorCode,
    Vec<LiveOutputEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (url, server) = serve_body(body).await;
    let client = LiveHostClient::new(&url, limits(max_events)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(capacity);
    let error = client
        .follow_live_output(SESSION, sender)
        .await
        .expect_err("finite SSE must not claim terminal success");
    let mut events = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        events.push(event);
    }
    (error.code, events, server)
}

#[tokio::test]
async fn malformed_version_kind_fields_and_identities_are_rejected() {
    let mut cases = Vec::new();
    let mut version = wire(STREAM_A, 1, "text_delta");
    version["api_version"] = json!("v2");
    cases.push(version);
    cases.push(wire(STREAM_A, 1, "future_kind"));
    let mut null_text = wire(STREAM_A, 1, "text_delta");
    null_text["text"] = Value::Null;
    cases.push(null_text);
    let mut forbidden = wire(STREAM_A, 1, "text_delta");
    forbidden["reason"] = json!("failed");
    cases.push(forbidden);
    let mut unknown = wire(STREAM_A, 1, "text_delta");
    unknown["secret"] = json!("must-not-pass");
    cases.push(unknown);
    let mut wrong_session = wire(STREAM_A, 1, "text_delta");
    wrong_session["session_id"] = json!("session-other");
    cases.push(wrong_session);
    cases.push(wire("not-a-uuid", 1, "text_delta"));

    for value in cases {
        let (code, events, server) = follow(live_body(&[value]), 4, 8).await;
        assert_eq!(code, HostClientErrorCode::InvalidEvent);
        assert!(events.is_empty());
        server.await.unwrap();
    }
}

#[tokio::test]
async fn sequence_gap_and_late_event_after_end_are_rejected() {
    for values in [
        vec![
            wire(STREAM_A, 1, "text_delta"),
            wire(STREAM_A, 3, "text_delta"),
        ],
        vec![
            wire(STREAM_A, 1, "text_delta"),
            wire(STREAM_A, 2, "ended"),
            wire(STREAM_A, 3, "text_delta"),
        ],
    ] {
        let (code, events, server) = follow(live_body(&values), 8, 8).await;
        assert_eq!(code, HostClientErrorCode::EventOrderViolation);
        assert!(!events.is_empty());
        server.await.unwrap();
    }
}

#[tokio::test]
async fn stream_replacement_requires_end_and_fresh_sequence_one() {
    let invalid = vec![
        wire(STREAM_A, 1, "text_delta"),
        wire(STREAM_B, 1, "text_delta"),
    ];
    let (code, _, server) = follow(live_body(&invalid), 8, 8).await;
    assert_eq!(code, HostClientErrorCode::EventOrderViolation);
    server.await.unwrap();

    let valid = vec![
        wire(STREAM_A, 1, "text_delta"),
        wire(STREAM_A, 2, "ended"),
        wire(STREAM_B, 1, "text_delta"),
    ];
    let (code, events, server) = follow(live_body(&valid), 8, 8).await;
    assert_eq!(code, HostClientErrorCode::TransportFailure);
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[2].kind,
        LiveOutputEventKind::TextDelta { .. }
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn bounded_sink_and_event_count_overflow_are_explicit() {
    let values = vec![
        wire(STREAM_A, 1, "text_delta"),
        wire(STREAM_A, 2, "text_delta"),
    ];
    let body = live_body(&values);
    let (code, events, server) = follow(body.clone(), 1, 8).await;
    assert_eq!(code, HostClientErrorCode::EventLimitExceeded);
    assert_eq!(events.len(), 1);
    server.await.unwrap();

    let (code, events, server) = follow(body, 8, 1).await;
    assert_eq!(code, HostClientErrorCode::EventLimitExceeded);
    assert_eq!(events.len(), 1);
    server.await.unwrap();
}

#[tokio::test]
async fn dropping_live_follow_closes_the_incomplete_sse_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (closed_sender, closed_receiver) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let body = live_body(&[wire(STREAM_A, 1, "text_delta")]);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let mut byte = [0_u8; 1];
        let closed = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut byte))
            .await
            .is_ok_and(|result| matches!(result, Ok(0)));
        let _ = closed_sender.send(closed);
    });
    let client = LiveHostClient::new(&format!("http://{address}/"), limits(8)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    let follow = tokio::spawn(async move { client.follow_live_output(SESSION, sender).await });
    receiver.recv().await.expect("first live event");
    follow.abort();
    assert!(follow.await.unwrap_err().is_cancelled());
    assert!(closed_receiver.await.unwrap(), "server did not observe EOF");
    server.await.unwrap();
}

async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        assert_eq!(socket.read(&mut byte).await.unwrap(), 1);
        request.push(byte[0]);
    }
    assert!(String::from_utf8(request)
        .unwrap()
        .starts_with("GET /v1/sessions/session-live/live HTTP/1.1\r\n"));
}
