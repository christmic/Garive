use futures::{SinkExt, StreamExt};
use garive_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport};
use garive_runtime::{
    BrowserPageId, BrowserSessionId, CdpBrowserSessionMode, CdpNativeAdapterPort,
    NativeActionCommandV1, NativeActionId, NativeAdapterPort, NativeObservationBounds,
    NativeSnapshotId, NativeTarget,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn reply(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    result: Value,
) -> Value {
    let Message::Text(message) = socket.next().await.expect("command").expect("frame") else {
        panic!("text command required")
    };
    let command: Value = serde_json::from_slice(message.as_bytes()).expect("command json");
    let mut response = json!({"id":command["id"],"result":result});
    if let Some(session_id) = command.get("sessionId") {
        response["sessionId"] = session_id.clone();
    }
    socket
        .send(Message::Text(response.to_string().into()))
        .await
        .expect("response");
    command
}

fn target() -> NativeTarget {
    NativeTarget::Browser {
        session_id: BrowserSessionId::new("browser-1").expect("browser"),
        page_id: BrowserPageId::new("page-1").expect("page"),
    }
}

fn history(id: i64, url: &str) -> Value {
    json!({"currentIndex":0,"entries":[{"id":id,"url":url}]})
}

async fn frames(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    origin: &str,
) {
    let command = reply(
        socket,
        json!({"frameTree":{"frame":{
            "id":"frame-main",
            "loaderId":"loader-main",
            "url":format!("{origin}/page"),
            "securityOrigin":origin,
            "mimeType":"text/html"
        }}}),
    )
    .await;
    assert_eq!(command["method"], "Page.getFrameTree");
}

fn frame_tree(origin: &str, loader_id: &str) -> Value {
    json!({"frameTree":{"frame":{
        "id":"frame-main",
        "loaderId":loader_id,
        "url":format!("{origin}/page"),
        "securityOrigin":origin,
        "mimeType":"text/html"
    }}})
}

async fn classify_text_input(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    backend_node_id: u64,
) {
    let command = reply(
        socket,
        json!({"node":{"localName":"input","attributes":["type","text"]}}),
    )
    .await;
    assert_eq!(command["method"], "DOM.describeNode");
    assert_eq!(command["params"]["backendNodeId"], backend_node_id);
}

async fn classify_password_input(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    backend_node_id: u64,
) {
    let command = reply(
        socket,
        json!({"node":{"localName":"input","attributes":["type","password"]}}),
    )
    .await;
    assert_eq!(command["method"], "DOM.describeNode");
    assert_eq!(command["params"]["backendNodeId"], backend_node_id);
}

async fn enable_managed_popups(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let page = reply(socket, json!({})).await;
    assert_eq!(page["method"], "Page.enable");
    assert_eq!(page["sessionId"], "cdp-session");
    let discovery = reply(socket, json!({})).await;
    assert_eq!(discovery["method"], "Target.setDiscoverTargets");
    assert!(discovery.get("sessionId").is_none());
}

async fn resulting_frames_with_popup(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let Message::Text(command) = socket.next().await.expect("frame tree").expect("frame") else {
        panic!("text command required")
    };
    let command: Value = serde_json::from_slice(command.as_bytes()).expect("command json");
    assert_eq!(command["method"], "Page.getFrameTree");
    socket
        .send(Message::Text(
            json!({
                "method":"Page.windowOpen",
                "params":{"url":"https://popup.test:443/start","windowName":"child","windowFeatures":[],"userGesture":true},
                "sessionId":"cdp-session"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("window open");
    socket
        .send(Message::Text(
            json!({
                "method":"Target.targetCreated",
                "params":{"targetInfo":{"targetId":"popup-page","type":"page","title":"","url":"","attached":false,"openerId":"page-1"}}
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("target created");
    socket
        .send(Message::Text(
            json!({
                "id":command["id"],
                "result":frame_tree("https://fixture.test:443", "loader-main"),
                "sessionId":"cdp-session"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("frame result");
}

async fn run_managed_popup_case(allow_popup: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        enable_managed_popups(&mut socket).await;
        assert_eq!(
            reply(&mut socket, json!({})).await["method"],
            "Accessibility.enable"
        );
        frames(&mut socket, "https://fixture.test:443").await;
        let tree = reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"button","ignored":false,"role":{"value":"button"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        assert_eq!(tree["method"], "Accessibility.getFullAXTree");
        assert_eq!(
            reply(&mut socket, history(1, "https://fixture.test:443/form")).await["method"],
            "Page.getNavigationHistory"
        );
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, json!({})).await;
        reply(
            &mut socket,
            json!({"model":{"content":[0,0,20,0,20,20,0,20]}}),
        )
        .await;
        for _ in 0..3 {
            reply(&mut socket, json!({})).await;
        }
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        resulting_frames_with_popup(&mut socket).await;
        if !allow_popup {
            let close = reply(&mut socket, json!({"success":true})).await;
            assert_eq!(close["method"], "Target.closeTarget");
            assert_eq!(close["params"]["targetId"], "popup-page");
        }
        let activate = reply(&mut socket, json!({})).await;
        assert_eq!(activate["method"], "Target.activateTarget");
        assert_eq!(activate["params"]["targetId"], "page-1");
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 32, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new_with_mode(
        target(),
        CdpBrowserSessionMode::Managed,
        "cdp-session",
        "revision-1",
        "run-popup",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let button = observation
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .expect("button");
    let allowed_origins = if allow_popup {
        json!(["https://popup.test:443"])
    } else {
        json!([])
    };
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-popup").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"click",
            "node_ref":button.node_ref.as_str(),
            "allowed_navigation_origins":allowed_origins
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("receipt");
    if allow_popup {
        assert_eq!(receipt.terminal_classification, "completed");
        let pending = port.take_pending_popup().expect("pending popup");
        assert_eq!(pending.page_id.as_str(), "popup-page");
        assert_eq!(pending.requested_origin, "https://popup.test:443");
        assert!(pending.user_gesture);
        assert!(port.take_pending_popup().is_none());
    } else {
        assert_eq!(receipt.terminal_classification, "failed");
        assert_eq!(
            receipt.failure_code.as_deref(),
            Some("browser_origin_denied")
        );
        assert!(port.take_pending_popup().is_none());
    }
    server.await.expect("server");
}

#[tokio::test]
async fn managed_popup_is_pending_until_separate_page_admission() {
    run_managed_popup_case(true).await;
}

#[tokio::test]
async fn managed_popup_outside_allowed_origins_is_closed() {
    run_managed_popup_case(false).await;
}

fn histories(current_index: usize) -> Value {
    json!({"currentIndex":current_index,"entries":[
        {"id":1,"url":"https://one.test:443/page"},
        {"id":2,"url":"https://two.test:443/page"}
    ]})
}

#[tokio::test]
async fn frame_navigation_during_semantic_observation_rejects_the_mixed_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        reply(
            &mut socket,
            frame_tree("https://fixture.test:443", "loader-before"),
        )
        .await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/page")).await;
        reply(
            &mut socket,
            frame_tree("https://fixture.test:443", "loader-after"),
        )
        .await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-frame-race",
        64,
        client,
    )
    .expect("port");
    assert_eq!(
        port.observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await,
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn concrete_port_dispatches_bound_click_type_and_clear_actions() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        assert_eq!(
            reply(&mut socket, json!({})).await["method"],
            "Accessibility.enable"
        );
        frames(&mut socket, "https://fixture.test:443").await;
        let tree = reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"button","ignored":false,"role":{"value":"button"},"name":{"value":"Submit"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"backendDOMNodeId":43,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"Fixture"},"childIds":["button","textbox"]}
            ]}),
        )
        .await;
        assert_eq!(tree["method"], "Accessibility.getFullAXTree");
        assert_eq!(
            reply(&mut socket, history(1, "https://fixture.test:443/form")).await["method"],
            "Page.getNavigationHistory"
        );
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(
            reply(&mut socket, json!({})).await["method"],
            "DOM.scrollIntoViewIfNeeded"
        );
        reply(
            &mut socket,
            json!({"model":{"content":[0,0,20,0,20,20,0,20]}}),
        )
        .await;
        for expected in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let command = reply(&mut socket, json!({})).await;
            assert_eq!(command["params"]["type"], expected);
        }
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(
            &mut socket,
            frame_tree("https://fixture.test:443", "loader-after-click"),
        )
        .await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"backendDOMNodeId":43,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(reply(&mut socket, json!({})).await["method"], "DOM.focus");
        let insert = reply(&mut socket, json!({})).await;
        assert_eq!(insert["method"], "Input.insertText");
        assert_eq!(insert["params"]["text"], "Garive 🦀");
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"value":{"value":"Garive 🦀"},"backendDOMNodeId":43,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(reply(&mut socket, json!({})).await["method"], "DOM.focus");
        for _ in 0..3 {
            assert_eq!(
                reply(&mut socket, json!({})).await["method"],
                "Input.dispatchKeyEvent"
            );
        }
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port =
        CdpNativeAdapterPort::new(target(), "cdp-session", "revision-1", "run-1", 64, client)
            .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let button = observation
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .expect("button");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-1").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id.clone(),
        target_revision: observation.target_revision.clone(),
        prepared_input: json!({"action":"click","node_ref":button.node_ref.as_str(),"allowed_navigation_origins":[]}),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("receipt");
    assert_eq!(receipt.prior_snapshot_id, observation.snapshot_id);
    assert!(receipt.resulting_snapshot_id.is_none());
    assert_eq!(receipt.terminal_classification, "completed");
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    assert_eq!(
        port.observe(
            &target(),
            Some(&NativeSnapshotId::new("stale").expect("snapshot")),
            observation.bounds,
        )
        .await,
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    let text_observation = port
        .observe(
            &target(),
            Some(&observation.snapshot_id),
            observation.bounds,
        )
        .await
        .expect("text observation");
    assert_ne!(
        text_observation.target_revision,
        observation.target_revision
    );
    let textbox_ref = text_observation
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .expect("textbox")
        .node_ref
        .clone();
    let type_command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-type").expect("action"),
        target: target(),
        expected_snapshot_id: text_observation.snapshot_id.clone(),
        target_revision: text_observation.target_revision.clone(),
        prepared_input: json!({
            "action":"type_text",
            "node_ref":textbox_ref.as_str(),
            "text":"Garive 🦀",
            "allowed_navigation_origins":[]
        }),
    };
    let type_binding = port
        .preflight_action(&type_command)
        .expect("type preflight");
    port.dispatch_action(&type_command, &type_binding)
        .await
        .expect("type receipt");
    let clear_observation = port
        .observe(
            &target(),
            Some(&text_observation.snapshot_id),
            text_observation.bounds,
        )
        .await
        .expect("clear observation");
    let clear_ref = clear_observation
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .expect("textbox")
        .node_ref
        .clone();
    let clear_command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-clear").expect("action"),
        target: target(),
        expected_snapshot_id: clear_observation.snapshot_id,
        target_revision: clear_observation.target_revision,
        prepared_input: json!({"action":"clear","node_ref":clear_ref.as_str(),"allowed_navigation_origins":[]}),
    };
    let clear_binding = port
        .preflight_action(&clear_command)
        .expect("clear preflight");
    port.dispatch_action(&clear_command, &clear_binding)
        .await
        .expect("clear receipt");
    server.await.expect("server");
}

#[tokio::test]
async fn concrete_port_selects_one_bound_native_option_with_effect_evidence() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        assert_eq!(
            reply(&mut socket, json!({})).await["method"],
            "Accessibility.enable"
        );
        frames(&mut socket, "https://fixture.test:443").await;
        let tree = reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"select","ignored":false,"role":{"value":"combobox"},"name":{"value":"Mode"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"},"childIds":["select"]}
            ]}),
        )
        .await;
        assert_eq!(tree["method"], "Accessibility.getFullAXTree");
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        let resolve = reply(
            &mut socket,
            json!({"object":{"type":"object","subtype":"node","objectId":"select-42"}}),
        )
        .await;
        assert_eq!(resolve["method"], "DOM.resolveNode");
        let select = reply(
            &mut socket,
            json!({"result":{"type":"object","value":{"status":"selected","changed":true,"value":"stable"}}}),
        )
        .await;
        assert_eq!(select["method"], "Runtime.callFunctionOn");
        assert_eq!(select["params"]["arguments"], json!([{"value":"stable"}]));
        assert_eq!(
            reply(&mut socket, json!({})).await["method"],
            "Runtime.releaseObject"
        );
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-select",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let node_ref = observation
        .nodes
        .iter()
        .find(|node| node.role == "combobox")
        .expect("combobox")
        .node_ref
        .clone();
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-select").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id.clone(),
        target_revision: observation.target_revision.clone(),
        prepared_input: json!({
            "action":"select_option",
            "node_ref":node_ref.as_str(),
            "option":"stable",
            "allowed_navigation_origins":[]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");

    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("receipt");

    assert_eq!(receipt.terminal_classification, "completed");
    assert!(receipt.failure_code.is_none());
    server.await.expect("server");
}

#[tokio::test]
async fn concrete_port_revalidates_focus_for_keys_and_binds_scroll_to_the_page_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        let focused_tree = json!({"nodes":[
            {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"backendDOMNodeId":43,"properties":[{"name":"focused","value":{"type":"booleanOrUndefined","value":true}}],"parentId":"root"},
            {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
        ]});
        reply(&mut socket, focused_tree.clone()).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        assert_eq!(
            reply(&mut socket, focused_tree.clone()).await["method"],
            "Accessibility.getFullAXTree"
        );
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        for expected in ["rawKeyDown", "keyUp"] {
            let key = reply(&mut socket, json!({})).await;
            assert_eq!(key["method"], "Input.dispatchKeyEvent");
            assert_eq!(key["params"]["type"], expected);
            assert_eq!(key["params"]["key"], "ArrowDown");
        }
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, focused_tree).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        let metrics = reply(
            &mut socket,
            json!({
                "visualViewport":{"pageX":0,"pageY":0,"clientWidth":1000,"clientHeight":600},
                "contentSize":{"width":1000,"height":1800}
            }),
        )
        .await;
        assert_eq!(metrics["method"], "Page.getLayoutMetrics");
        let scroll = reply(&mut socket, json!({})).await;
        assert_eq!(scroll["method"], "Input.dispatchMouseEvent");
        assert_eq!(scroll["params"]["type"], "mouseWheel");
        assert_eq!(scroll["params"]["x"], 500.0);
        assert_eq!(scroll["params"]["y"], 300.0);
        assert_eq!(scroll["params"]["deltaY"], 120);
        let settled = reply(
            &mut socket,
            json!({
                "visualViewport":{"pageX":0,"pageY":120,"clientWidth":1000,"clientHeight":600},
                "contentSize":{"width":1000,"height":1800}
            }),
        )
        .await;
        assert_eq!(settled["method"], "Page.getLayoutMetrics");
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-key-scroll",
        64,
        client,
    )
    .expect("port");
    let before = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    assert!(before.focused_node.is_some());
    let key = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-key").expect("action"),
        target: target(),
        expected_snapshot_id: before.snapshot_id.clone(),
        target_revision: before.target_revision.clone(),
        prepared_input: json!({"action":"press_key","key":"arrow_down","allowed_navigation_origins":[]}),
    };
    let key_binding = port.preflight_action(&key).expect("key preflight");
    port.dispatch_action(&key, &key_binding)
        .await
        .expect("key receipt");
    let after_key = port
        .observe(&target(), Some(&before.snapshot_id), before.bounds)
        .await
        .expect("post-key observation");
    let scroll = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-scroll").expect("action"),
        target: target(),
        expected_snapshot_id: after_key.snapshot_id,
        target_revision: after_key.target_revision,
        prepared_input: json!({"action":"scroll","delta_x":0,"delta_y":120,"allowed_navigation_origins":[]}),
    };
    let scroll_binding = port.preflight_action(&scroll).expect("scroll preflight");
    port.dispatch_action(&scroll, &scroll_binding)
        .await
        .expect("scroll receipt");
    server.await.expect("server");
}

#[tokio::test]
async fn key_dispatch_fails_before_input_when_focus_changed_after_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"one","ignored":false,"role":{"value":"textbox"},"backendDOMNodeId":43,"properties":[{"name":"focused","value":{"value":true}}],"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"two","ignored":false,"role":{"value":"textbox"},"backendDOMNodeId":44,"properties":[{"name":"focused","value":{"value":true}}],"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-focus-change",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-focus-change").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({"action":"press_key","key":"enter","allowed_navigation_origins":[]}),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    assert_eq!(
        port.dispatch_action(&command, &binding).await,
        Err(garive_runtime::NativeProtocolError::FocusChanged)
    );
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn text_field_becoming_password_after_snapshot_fails_before_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"backendDOMNodeId":43,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        classify_password_input(&mut socket, 43).await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-password-race",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let textbox = observation
        .nodes
        .iter()
        .find(|node| node.role == "textbox")
        .expect("textbox");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("password-race").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"type_text",
            "node_ref":textbox.node_ref.as_str(),
            "text":"must-not-send",
            "allowed_navigation_origins":[]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    assert_eq!(
        port.dispatch_action(&command, &binding).await,
        Err(garive_runtime::NativeProtocolError::SensitiveActionRequired)
    );
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn action_navigation_revalidates_the_committed_history_origin() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        let focused_tree = json!({"nodes":[
            {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"backendDOMNodeId":43,"properties":[{"name":"focused","value":{"value":true}}],"parentId":"root"},
            {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
        ]});
        reply(&mut socket, focused_tree.clone()).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        classify_text_input(&mut socket, 43).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, focused_tree).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, json!({})).await;
        reply(&mut socket, json!({})).await;
        reply(&mut socket, history(2, "https://denied.test:443/landing")).await;
        frames(&mut socket, "https://denied.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-action-navigation",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-navigation-denied").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"press_key",
            "key":"enter",
            "allowed_navigation_origins":[]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("failed receipt");
    assert_eq!(receipt.terminal_classification, "failed");
    assert_eq!(
        receipt.failure_code.as_deref(),
        Some("browser_origin_denied")
    );
    receipt.validate().expect("valid receipt");
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn history_back_moves_only_to_a_prevalidated_origin() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://two.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, histories(1)).await;
        frames(&mut socket, "https://two.test:443").await;
        frames(&mut socket, "https://two.test:443").await;
        reply(&mut socket, histories(1)).await;
        let movement = reply(&mut socket, json!({})).await;
        assert_eq!(movement["method"], "Page.navigateToHistoryEntry");
        assert_eq!(movement["params"]["entryId"], 1);
        reply(&mut socket, histories(0)).await;
        frames(&mut socket, "https://one.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-history-back",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("history-back").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"go_back",
            "allowed_navigation_origins":["https://one.test:443"]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("receipt");
    assert_eq!(receipt.terminal_classification, "completed");
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn history_forward_denial_returns_a_receipt_without_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://one.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, histories(0)).await;
        frames(&mut socket, "https://one.test:443").await;
        frames(&mut socket, "https://one.test:443").await;
        reply(&mut socket, histories(0)).await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-history-denied",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("history-forward-denied").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({"action":"go_forward","allowed_navigation_origins":[]}),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("failed receipt");
    assert_eq!(receipt.terminal_classification, "failed");
    assert_eq!(
        receipt.failure_code.as_deref(),
        Some("browser_origin_denied")
    );
    assert_eq!(port.preflight_action(&command), Ok(binding));
    server.await.expect("server");
}

#[tokio::test]
async fn reload_waits_for_load_and_rotates_the_snapshot_revision() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://one.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, histories(0)).await;
        frames(&mut socket, "https://one.test:443").await;
        frames(&mut socket, "https://one.test:443").await;
        reply(&mut socket, histories(0)).await;
        assert_eq!(reply(&mut socket, json!({})).await["method"], "Page.enable");
        assert_eq!(reply(&mut socket, json!({})).await["method"], "Page.reload");
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("load event");
        reply(&mut socket, histories(0)).await;
        frames(&mut socket, "https://one.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-reload",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("history-reload").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"reload",
            "allowed_navigation_origins":["https://one.test:443"]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("receipt");
    assert_eq!(receipt.terminal_classification, "completed");
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn external_history_change_makes_the_observed_snapshot_stale_before_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"button","ignored":false,"role":{"value":"button"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(2, "https://fixture.test:443/changed")).await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-external-navigation",
        64,
        client,
    )
    .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let button = observation
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .expect("button");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("external-navigation-stale").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"click",
            "node_ref":button.node_ref.as_str(),
            "allowed_navigation_origins":[]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    assert_eq!(
        port.dispatch_action(&command, &binding).await,
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn dispatch_loss_is_uncertain_and_invalidates_the_old_snapshot_binding() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"button","ignored":false,"role":{"value":"button"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        let Message::Text(_) = socket.next().await.expect("dispatch").expect("frame") else {
            panic!("text dispatch required")
        };
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port =
        CdpNativeAdapterPort::new(target(), "cdp-session", "revision-1", "run-2", 64, client)
            .expect("port");
    let observation = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("observation");
    let button_ref = observation
        .nodes
        .iter()
        .find(|node| node.role == "button")
        .expect("button")
        .node_ref
        .clone();
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-2").expect("action"),
        target: target(),
        expected_snapshot_id: observation.snapshot_id,
        target_revision: observation.target_revision,
        prepared_input: json!({
            "action":"click",
            "node_ref":button_ref.as_str(),
            "allowed_navigation_origins":[]
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    assert_eq!(
        port.dispatch_action(&command, &binding).await,
        Err(garive_runtime::NativeProtocolError::ActionUncertain)
    );
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}

#[tokio::test]
async fn navigation_revalidates_committed_origin_and_rotates_target_revision() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(reply(&mut socket, json!({})).await["method"], "Page.enable");
        let Message::Text(message) = socket.next().await.expect("navigate").expect("frame") else {
            panic!("text navigation required")
        };
        let navigate: Value = serde_json::from_slice(message.as_bytes()).expect("navigation json");
        assert_eq!(navigate["method"], "Page.navigate");
        socket
            .send(Message::Text(
                json!({"method":"Page.frameNavigated","params":{"frame":{"id":"frame-main","url":"https://example.com:443/final"}},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("frame event");
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{"timestamp":1},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("load event");
        socket
            .send(Message::Text(
                json!({"id":navigate["id"],"result":{"frameId":"frame-main","loaderId":"loader-2"},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("navigate response");
        reply(&mut socket, history(2, "https://example.com:443/final")).await;
        frames(&mut socket, "https://example.com:443").await;
        frames(&mut socket, "https://example.com:443").await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"new-root","ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"Final"}}]}),
        )
        .await;
        reply(&mut socket, history(2, "https://example.com:443/final")).await;
        frames(&mut socket, "https://example.com:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port =
        CdpNativeAdapterPort::new(target(), "cdp-session", "revision-1", "run-nav", 64, client)
            .expect("port");
    let before = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("before navigation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-nav").expect("action"),
        target: target(),
        expected_snapshot_id: before.snapshot_id.clone(),
        target_revision: before.target_revision.clone(),
        prepared_input: json!({
            "destination_url":"https://example.com:443/start",
            "destination_origin":"https://example.com:443",
            "wait_until":"load",
            "timeout_ms":1_000,
            "max_nodes":16,
            "max_text_bytes":4_096
        }),
    };
    let binding = port
        .preflight_action(&command)
        .expect("navigation preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("navigation receipt");
    assert_eq!(receipt.terminal_classification, "completed");
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    let after = port
        .observe(&target(), Some(&before.snapshot_id), before.bounds)
        .await
        .expect("after navigation");
    assert_ne!(after.target_revision, before.target_revision);
    assert_eq!(after.nodes[0].name.as_deref(), Some("Final"));
    server.await.expect("server");
}

#[tokio::test]
async fn cross_origin_redirect_returns_a_failed_receipt_and_invalidates_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        frames(&mut socket, "https://fixture.test:443").await;
        frames(&mut socket, "https://fixture.test:443").await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, json!({})).await;
        let Message::Text(message) = socket.next().await.expect("navigate").expect("frame") else {
            panic!("text navigation required")
        };
        let navigate: Value = serde_json::from_slice(message.as_bytes()).expect("navigation json");
        socket
            .send(Message::Text(
                json!({"method":"Page.frameNavigated","params":{"frame":{"id":"frame-main","url":"https://denied.test:443/final"}},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("frame event");
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("load event");
        socket
            .send(Message::Text(
                json!({"id":navigate["id"],"result":{"frameId":"frame-main","loaderId":"loader-denied"},"sessionId":"cdp-session"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("navigate response");
        reply(&mut socket, history(2, "https://denied.test:443/final")).await;
        frames(&mut socket, "https://denied.test:443").await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let mut port = CdpNativeAdapterPort::new(
        target(),
        "cdp-session",
        "revision-1",
        "run-denied",
        64,
        client,
    )
    .expect("port");
    let before = port
        .observe(
            &target(),
            None,
            NativeObservationBounds {
                max_nodes: 16,
                max_text_bytes: 4_096,
            },
        )
        .await
        .expect("before navigation");
    let command = NativeActionCommandV1 {
        action_id: NativeActionId::new("action-denied").expect("action"),
        target: target(),
        expected_snapshot_id: before.snapshot_id,
        target_revision: before.target_revision,
        prepared_input: json!({
            "destination_url":"https://example.com:443/start",
            "destination_origin":"https://example.com:443",
            "wait_until":"load",
            "timeout_ms":1_000,
            "max_nodes":16,
            "max_text_bytes":4_096
        }),
    };
    let binding = port.preflight_action(&command).expect("preflight");
    let receipt = port
        .dispatch_action(&command, &binding)
        .await
        .expect("failed receipt");
    assert_eq!(receipt.terminal_classification, "failed");
    assert_eq!(
        receipt.failure_code.as_deref(),
        Some("browser_origin_denied")
    );
    receipt.validate().expect("valid failed receipt");
    assert_eq!(
        port.preflight_action(&command),
        Err(garive_runtime::NativeProtocolError::SnapshotStale)
    );
    server.await.expect("server");
}
