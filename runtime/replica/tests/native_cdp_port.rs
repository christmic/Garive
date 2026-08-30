use futures::{SinkExt, StreamExt};
use garive_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport};
use garive_runtime::{
    BrowserPageId, BrowserSessionId, CdpNativeAdapterPort, NativeActionCommandV1, NativeActionId,
    NativeAdapterPort, NativeObservationBounds, NativeSnapshotId, NativeTarget,
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
    socket
        .send(Message::Text(
            json!({"id":command["id"],"result":result,"sessionId":"cdp-session"})
                .to_string()
                .into(),
        ))
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

fn histories(current_index: usize) -> Value {
    json!({"currentIndex":current_index,"entries":[
        {"id":1,"url":"https://one.test:443/page"},
        {"id":2,"url":"https://two.test:443/page"}
    ]})
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
            json!({"nodes":[
                {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"backendDOMNodeId":43,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(reply(&mut socket, json!({})).await["method"], "DOM.focus");
        let insert = reply(&mut socket, json!({})).await;
        assert_eq!(insert["method"], "Input.insertText");
        assert_eq!(insert["params"]["text"], "Garive 🦀");
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"value":{"value":"Garive 🦀"},"backendDOMNodeId":43,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(reply(&mut socket, json!({})).await["method"], "DOM.focus");
        for _ in 0..3 {
            assert_eq!(
                reply(&mut socket, json!({})).await["method"],
                "Input.dispatchKeyEvent"
            );
        }
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
async fn concrete_port_revalidates_focus_for_keys_and_binds_scroll_to_the_page_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        let focused_tree = json!({"nodes":[
            {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"name":{"value":"Account"},"backendDOMNodeId":43,"properties":[{"name":"focused","value":{"type":"booleanOrUndefined","value":true}}],"parentId":"root"},
            {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
        ]});
        reply(&mut socket, focused_tree.clone()).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        assert_eq!(
            reply(&mut socket, focused_tree.clone()).await["method"],
            "Accessibility.getFullAXTree"
        );
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        for expected in ["rawKeyDown", "keyUp"] {
            let key = reply(&mut socket, json!({})).await;
            assert_eq!(key["method"], "Input.dispatchKeyEvent");
            assert_eq!(key["params"]["type"], expected);
            assert_eq!(key["params"]["key"], "ArrowDown");
        }
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, focused_tree).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"one","ignored":false,"role":{"value":"textbox"},"backendDOMNodeId":43,"properties":[{"name":"focused","value":{"value":true}}],"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
async fn action_navigation_revalidates_the_committed_history_origin() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        reply(&mut socket, json!({})).await;
        let focused_tree = json!({"nodes":[
            {"nodeId":"textbox","ignored":false,"role":{"value":"textbox"},"backendDOMNodeId":43,"properties":[{"name":"focused","value":{"value":true}}],"parentId":"root"},
            {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
        ]});
        reply(&mut socket, focused_tree.clone()).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, focused_tree).await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
        reply(&mut socket, json!({})).await;
        reply(&mut socket, json!({})).await;
        reply(&mut socket, history(2, "https://denied.test:443/landing")).await;
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
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, histories(1)).await;
        reply(&mut socket, histories(1)).await;
        let movement = reply(&mut socket, json!({})).await;
        assert_eq!(movement["method"], "Page.navigateToHistoryEntry");
        assert_eq!(movement["params"]["entryId"], 1);
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
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, histories(0)).await;
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
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, histories(0)).await;
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
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"button","ignored":false,"role":{"value":"button"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
        reply(
            &mut socket,
            json!({"nodes":[
                {"nodeId":"button","ignored":false,"role":{"value":"button"},"backendDOMNodeId":42,"parentId":"root"},
                {"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}
            ]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"new-root","ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"Final"}}]}),
        )
        .await;
        reply(&mut socket, history(2, "https://example.com:443/final")).await;
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
        reply(
            &mut socket,
            json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"RootWebArea"}}]}),
        )
        .await;
        reply(&mut socket, history(1, "https://fixture.test:443/form")).await;
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
