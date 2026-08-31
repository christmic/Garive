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
        let frames = reply(
            &mut socket,
            json!({"frameTree":{
                "frame":{"id":"frame-1","loaderId":"loader-1","url":"https://example.test:443/current","securityOrigin":"https://example.test:443","mimeType":"text/html"},
                "childFrames":[
                    {"frame":{"id":"frame-2","parentId":"frame-1","loaderId":"loader-2","url":"https://example.test:443/child","securityOrigin":"https://example.test:443","mimeType":"text/html"}},
                    {"frame":{"id":"frame-3","parentId":"frame-1","loaderId":"loader-3","url":"https://other.test:443/child","securityOrigin":"https://other.test:443","mimeType":"text/html"}}
                ]
            }}),
        )
        .await;
        assert_eq!(frames["method"], "Page.getFrameTree");
        let owner = reply(&mut socket, json!({"backendNodeId":84})).await;
        assert_eq!(owner["method"], "DOM.getFrameOwner");
        assert_eq!(owner["params"]["frameId"], "frame-2");
        let password = reply(
            &mut socket,
            json!({"node":{"localName":"input","attributes":["type","password","value","must-stay-private"]}}),
        )
        .await;
        assert_eq!(password["method"], "DOM.describeNode");
        assert_eq!(password["params"]["backendNodeId"], 84);
        assert_eq!(password["params"]["depth"], 0);
        assert_eq!(password["params"]["pierce"], false);
        let history = reply(
            &mut socket,
            json!({"currentIndex":1,"entries":[
                {"id":1,"url":"https://example.test:443/old","userTypedURL":"","title":"Old","transitionType":"typed"},
                {"id":2,"url":"https://example.test:443/current","userTypedURL":"","title":"Current","transitionType":"link"}
            ]}),
        )
        .await;
        assert_eq!(history["method"], "Page.getNavigationHistory");
        let malformed = reply(
            &mut socket,
            json!({"frameTree":{
                "frame":{"id":"frame-1","loaderId":"loader-1","url":"about:blank","securityOrigin":"://"},
                "childFrames":[{"frame":{"id":"frame-2","parentId":"wrong-parent","loaderId":"loader-2","url":"about:blank","securityOrigin":"://"}}]
            }}),
        )
        .await;
        assert_eq!(malformed["method"], "Page.getFrameTree");
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
    let frames = client.frame_tree(&session).await.expect("frame tree");
    assert_eq!(frames.main_frame_id, "frame-1");
    assert_eq!(frames.frames.len(), 3);
    assert_eq!(frames.frames[1].parent_id.as_deref(), Some("frame-1"));
    assert_eq!(frames.frames[2].security_origin, "https://other.test:443");
    assert_eq!(
        client
            .frame_owner_backend_node(&session, "frame-2")
            .await
            .expect("frame owner"),
        84
    );
    assert!(client
        .backend_node_is_password(&session, 84)
        .await
        .expect("password classification"));
    let history = client
        .current_history_entry(&session)
        .await
        .expect("history");
    assert_eq!(history.id, 2);
    assert_eq!(history.url, "https://example.test:443/current");
    assert!(client.frame_tree(&session).await.is_err());
    server.await.expect("server");
}

#[tokio::test]
async fn history_move_and_reload_prove_the_exact_current_entry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let entries = json!([
            {"id":1,"url":"https://example.test:443/one"},
            {"id":2,"url":"https://example.test:443/two"},
            {"id":3,"url":"https://example.test:443/three"}
        ]);
        let initial = reply(
            &mut socket,
            json!({"currentIndex":1,"entries":entries.clone()}),
        )
        .await;
        assert_eq!(initial["method"], "Page.getNavigationHistory");
        let move_command = reply(&mut socket, json!({})).await;
        assert_eq!(move_command["method"], "Page.navigateToHistoryEntry");
        assert_eq!(move_command["params"]["entryId"], 1);
        reply(
            &mut socket,
            json!({"currentIndex":0,"entries":entries.clone()}),
        )
        .await;

        let Message::Text(enable) = socket.next().await.expect("enable").expect("frame") else {
            panic!("text command required")
        };
        let enable: Value = serde_json::from_slice(enable.as_bytes()).expect("enable json");
        assert_eq!(enable["method"], "Page.enable");
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{},"sessionId":"session-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("stale load event");
        socket
            .send(Message::Text(
                json!({"id":enable["id"],"result":{},"sessionId":"session-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("enable response");
        let reload = reply(&mut socket, json!({})).await;
        assert_eq!(reload["method"], "Page.reload");
        assert_eq!(reload["params"]["ignoreCache"], false);
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{},"sessionId":"session-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("fresh load event");
        reply(&mut socket, json!({"currentIndex":0,"entries":entries})).await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let history = client
        .navigation_history("session-1")
        .await
        .expect("history");
    assert_eq!(history.current_index, 1);
    assert_eq!(history.entries.len(), 3);
    let moved = client
        .navigate_to_history_entry("session-1", 1)
        .await
        .expect("history move");
    assert_eq!(moved.id, 1);
    let reloaded = client.reload("session-1").await.expect("reload");
    assert_eq!(reloaded.id, 1);
    server.await.expect("server");
}
