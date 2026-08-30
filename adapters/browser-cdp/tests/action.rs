use futures::{SinkExt, StreamExt};
use garive_adapter_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn next(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    result: Value,
) -> Value {
    let Message::Text(message) = socket.next().await.expect("command").expect("frame") else {
        panic!("text command required")
    };
    let command: Value = serde_json::from_slice(message.as_bytes()).expect("command json");
    socket
        .send(Message::Text(
            json!({"id":command["id"],"result":result,"sessionId":"session-1"})
                .to_string()
                .into(),
        ))
        .await
        .expect("response");
    command
}

#[tokio::test]
async fn semantic_click_resolves_current_box_and_emits_one_exact_pointer_sequence() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let scroll = next(&mut socket, json!({})).await;
        assert_eq!(scroll["method"], "DOM.scrollIntoViewIfNeeded");
        assert_eq!(scroll["params"]["backendNodeId"], 42);
        let box_model = next(
            &mut socket,
            json!({"model":{"content":[10,20,30,20,30,40,10,40]}}),
        )
        .await;
        assert_eq!(box_model["method"], "DOM.getBoxModel");
        for expected in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let input = next(&mut socket, json!({})).await;
            assert_eq!(input["method"], "Input.dispatchMouseEvent");
            assert_eq!(input["params"]["type"], expected);
            assert_eq!(input["params"]["x"], 20.0);
            assert_eq!(input["params"]["y"], 30.0);
        }
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    client
        .click_backend_node("session-1", 42)
        .await
        .expect("click");
    server.await.expect("server");
}
