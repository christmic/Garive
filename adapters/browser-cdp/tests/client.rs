use futures::{SinkExt, StreamExt};
use garive_adapter_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn reply(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    result: Value,
) -> Value {
    let message = socket.next().await.expect("command").expect("frame");
    let Message::Text(message) = message else {
        panic!("text command required")
    };
    let command: Value = serde_json::from_slice(message.as_bytes()).expect("command json");
    let mut response = json!({"id":command["id"],"result":result});
    if let Some(session) = command.get("sessionId") {
        response["sessionId"] = session.clone();
    }
    socket
        .send(Message::Text(response.to_string().into()))
        .await
        .expect("response");
    command
}

#[tokio::test]
async fn typed_client_binds_version_target_session_and_bounded_ax_tree() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let version = reply(
            &mut socket,
            json!({"protocolVersion":"1.3","product":"Chrome/140","revision":"r1","userAgent":"redacted","jsVersion":"14.0"}),
        )
        .await;
        assert_eq!(version["method"], "Browser.getVersion");
        let create = reply(&mut socket, json!({"targetId":"target-1"})).await;
        assert_eq!(create["method"], "Target.createTarget");
        assert_eq!(create["params"]["url"], "about:blank");
        let attach = reply(&mut socket, json!({"sessionId":"target-session-1"})).await;
        assert_eq!(attach["method"], "Target.attachToTarget");
        assert_eq!(attach["params"]["flatten"], true);
        let enable = reply(&mut socket, json!({})).await;
        assert_eq!(enable["method"], "Accessibility.enable");
        assert_eq!(enable["sessionId"], "target-session-1");
        let tree = reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"ax-root","ignored":false,"role":{"type":"role","value":"RootWebArea"},"name":{"type":"computedString","value":"Example"},"childIds":["ax-button"],"frameId":"frame-1"},
                {"nodeId":"ax-button","ignored":false,"role":{"type":"role","value":"button"},"name":{"type":"computedString","value":"Submit"},"properties":[{"name":"focusable","value":{"type":"booleanOrUndefined","value":true}}],"parentId":"ax-root","backendDOMNodeId":42}
            ]}),
        )
        .await;
        assert_eq!(tree["method"], "Accessibility.getFullAXTree");
        assert_eq!(tree["params"]["depth"], 64);
        assert_eq!(tree["params"]["frameId"], "frame-1");
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let transport = CdpTransport::connect(&config).await.expect("transport");
    let mut client = CdpClient::new(transport);
    let version = client.browser_version().await.expect("version");
    assert_eq!(version.protocol_version, "1.3");
    let target = client.create_blank_target().await.expect("create");
    let session = client.attach_target(&target).await.expect("attach");
    client.enable_accessibility(&session).await.expect("enable");
    let tree = client
        .full_ax_tree(&session, Some("frame-1"), 64, 10, 4_096)
        .await
        .expect("tree");
    assert_eq!(tree.nodes.len(), 2);
    assert_eq!(tree.nodes[1].role.as_deref(), Some("button"));
    assert_eq!(tree.nodes[1].backend_dom_node_id, Some(42));
    server.await.expect("server");
}
