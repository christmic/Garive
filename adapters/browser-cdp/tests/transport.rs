use futures::{SinkExt, StreamExt};
use garive_adapter_browser_cdp::{
    CdpAdapterConfig, CdpIncoming, CdpLimits, CdpTransport, CdpTransportError,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn endpoint() -> (TcpListener, CdpAdapterConfig) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/test-capability"),
        CdpLimits::new(4_096, 1, 2, 2_000).expect("limits"),
    )
    .expect("config");
    (listener, config)
}

#[tokio::test]
async fn call_correlates_session_and_queues_bounded_events() {
    let (listener, config) = endpoint().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let command = socket.next().await.expect("command").expect("frame");
        let Message::Text(command) = command else {
            panic!("text command required")
        };
        let command: Value = serde_json::from_slice(command.as_bytes()).expect("json");
        socket
            .send(Message::Text(
                json!({"method":"Network.requestWillBeSent","params":{"requestId":"r1"},"sessionId":"target-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("unrelated event");
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{"timestamp":1},"sessionId":"target-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("event");
        socket
            .send(Message::Text(
                json!({"id":command["id"],"result":{"nodes":[]},"sessionId":"target-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("result");
    });
    let mut transport = CdpTransport::connect(&config).await.expect("connect");
    assert_eq!(
        transport
            .call(
                "Accessibility.getFullAXTree",
                json!({"depth":32}),
                Some("target-1".into())
            )
            .await,
        Ok(json!({"nodes":[]}))
    );
    assert_eq!(
        transport
            .wait_for_event("Page.loadEventFired", Some("target-1"))
            .await,
        Ok(json!({"timestamp":1}))
    );
    assert!(
        matches!(transport.pop_event(), Some(CdpIncoming::Event { method, .. }) if method == "Network.requestWillBeSent")
    );
    server.await.expect("server");
}

#[tokio::test]
async fn wrong_response_session_fails_closed() {
    let (listener, config) = endpoint().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let command = socket.next().await.expect("command").expect("frame");
        let Message::Text(command) = command else {
            panic!("text command required")
        };
        let command: Value = serde_json::from_slice(command.as_bytes()).expect("json");
        socket
            .send(Message::Text(
                json!({"id":command["id"],"result":{},"sessionId":"other-target"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("result");
    });
    let mut transport = CdpTransport::connect(&config).await.expect("connect");
    assert_eq!(
        transport
            .call("Accessibility.enable", json!({}), Some("target-1".into()))
            .await,
        Err(CdpTransportError::CorrelationMismatch)
    );
    server.await.expect("server");
}
