use futures::{SinkExt, StreamExt};
use garive_adapter_browser_cdp::{
    CdpAdapterConfig, CdpClient, CdpLimits, CdpPortableKey, CdpSelectOutcome, CdpTransport,
};
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
async fn select_option_uses_one_fixed_function_and_releases_the_exact_node() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let resolve = next(
            &mut socket,
            json!({"object":{"type":"object","subtype":"node","objectId":"select-42"}}),
        )
        .await;
        assert_eq!(resolve["method"], "DOM.resolveNode");
        assert_eq!(resolve["params"], json!({"backendNodeId":42}));

        let call = next(
            &mut socket,
            json!({"result":{"type":"object","value":{"status":"selected","changed":true,"value":"stable"}}}),
        )
        .await;
        assert_eq!(call["method"], "Runtime.callFunctionOn");
        assert_eq!(call["params"]["objectId"], "select-42");
        assert_eq!(call["params"]["arguments"], json!([{"value":"stable"}]));
        assert_eq!(call["params"]["returnByValue"], true);
        let declaration = call["params"]["functionDeclaration"]
            .as_str()
            .expect("fixed function");
        assert!(declaration.contains("HTMLSelectElement"));
        assert!(!declaration.contains("stable"));

        let release = next(&mut socket, json!({})).await;
        assert_eq!(release["method"], "Runtime.releaseObject");
        assert_eq!(release["params"], json!({"objectId":"select-42"}));

        next(
            &mut socket,
            json!({"object":{"type":"object","subtype":"node","objectId":"select-42"}}),
        )
        .await;
        next(
            &mut socket,
            json!({"result":{"type":"object","value":{"status":"unavailable"}}}),
        )
        .await;
        next(&mut socket, json!({})).await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));

    let outcome = client
        .select_option_backend_node("session-1", 42, "stable")
        .await
        .expect("select option");

    assert_eq!(outcome, CdpSelectOutcome::Selected { changed: true });
    assert_eq!(
        client
            .select_option_backend_node("session-1", 42, "missing")
            .await
            .expect("unavailable option"),
        CdpSelectOutcome::OptionUnavailable
    );
    server.await.expect("server");
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

#[tokio::test]
async fn text_actions_focus_the_exact_node_without_clipboard_or_script() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let focus = next(&mut socket, json!({})).await;
        assert_eq!(focus["method"], "DOM.focus");
        assert_eq!(focus["params"], json!({"backendNodeId":42}));
        let insert = next(&mut socket, json!({})).await;
        assert_eq!(insert["method"], "Input.insertText");
        assert_eq!(insert["params"], json!({"text":"Garive 🦀"}));
        let refocus = next(&mut socket, json!({})).await;
        assert_eq!(refocus["method"], "DOM.focus");
        let select_all = next(&mut socket, json!({})).await;
        assert_eq!(select_all["method"], "Input.dispatchKeyEvent");
        assert_eq!(
            select_all["params"],
            json!({"type":"rawKeyDown","commands":["selectAll"]})
        );
        for expected in ["rawKeyDown", "keyUp"] {
            let backspace = next(&mut socket, json!({})).await;
            assert_eq!(backspace["method"], "Input.dispatchKeyEvent");
            assert_eq!(backspace["params"]["type"], expected);
            assert_eq!(backspace["params"]["key"], "Backspace");
        }
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    client
        .type_text_backend_node("session-1", 42, "Garive 🦀")
        .await
        .expect("type text");
    client
        .clear_backend_node("session-1", 42)
        .await
        .expect("clear text");
    server.await.expect("server");
}

#[tokio::test]
async fn portable_key_and_viewport_scroll_use_only_closed_native_fields() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        for expected in ["rawKeyDown", "keyUp"] {
            let key = next(&mut socket, json!({})).await;
            assert_eq!(key["method"], "Input.dispatchKeyEvent");
            assert_eq!(key["params"]["type"], expected);
            assert_eq!(key["params"]["key"], "ArrowDown");
            assert_eq!(key["params"]["windowsVirtualKeyCode"], 40);
        }
        let metrics = next(
            &mut socket,
            json!({
                "visualViewport":{"pageX":0,"pageY":0,"clientWidth":1280,"clientHeight":720},
                "contentSize":{"width":1600,"height":2400}
            }),
        )
        .await;
        assert_eq!(metrics["method"], "Page.getLayoutMetrics");
        let scroll = next(&mut socket, json!({})).await;
        assert_eq!(scroll["method"], "Input.dispatchMouseEvent");
        assert_eq!(scroll["params"]["type"], "mouseWheel");
        assert_eq!(scroll["params"]["x"], 640.0);
        assert_eq!(scroll["params"]["y"], 360.0);
        assert_eq!(scroll["params"]["deltaX"], -5);
        assert_eq!(scroll["params"]["deltaY"], 80);
        let settled = next(
            &mut socket,
            json!({
                "visualViewport":{"pageX":0,"pageY":80,"clientWidth":1280,"clientHeight":720},
                "contentSize":{"width":1600,"height":2400}
            }),
        )
        .await;
        assert_eq!(settled["method"], "Page.getLayoutMetrics");
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    client
        .press_key("session-1", CdpPortableKey::ArrowDown)
        .await
        .expect("portable key");
    client
        .scroll_viewport("session-1", -5, 80)
        .await
        .expect("viewport scroll");
    server.await.expect("server");
}
