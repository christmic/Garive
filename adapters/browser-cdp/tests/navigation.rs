use futures::{SinkExt, StreamExt};
use garive_adapter_browser_cdp::{
    CdpAdapterConfig, CdpClient, CdpLimits, CdpNavigationOutcome, CdpTransport, CdpWaitUntil,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn command(socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Value {
    let Message::Text(message) = socket.next().await.expect("command").expect("frame") else {
        panic!("text command required")
    };
    serde_json::from_slice(message.as_bytes()).expect("command json")
}

async fn response(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    command: &Value,
    result: Value,
) {
    socket
        .send(Message::Text(
            json!({"id":command["id"],"result":result,"sessionId":"session-1"})
                .to_string()
                .into(),
        ))
        .await
        .expect("response");
}

#[tokio::test]
async fn navigation_waits_for_load_and_returns_the_committed_redirect_url() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let enable = command(&mut socket).await;
        assert_eq!(enable["method"], "Page.enable");
        response(&mut socket, &enable, json!({})).await;
        let navigate = command(&mut socket).await;
        assert_eq!(navigate["method"], "Page.navigate");
        socket
            .send(Message::Text(
                json!({"method":"Page.frameNavigated","params":{"frame":{"id":"frame-main","url":"https://final.test:443/form"}},"sessionId":"session-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("frame event");
        socket
            .send(Message::Text(
                json!({"method":"Page.loadEventFired","params":{"timestamp":1},"sessionId":"session-1"})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("load event");
        response(
            &mut socket,
            &navigate,
            json!({"frameId":"frame-main","loaderId":"loader-1"}),
        )
        .await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    let result = client
        .navigate(
            "session-1",
            "https://initial.test:443/start",
            CdpWaitUntil::Load,
        )
        .await
        .expect("navigation");
    assert_eq!(result.frame_id, "frame-main");
    assert_eq!(result.loader_id.as_deref(), Some("loader-1"));
    assert_eq!(
        result.outcome,
        CdpNavigationOutcome::Page {
            final_url: "https://final.test:443/form".into()
        }
    );
    server.await.expect("server");
}

#[tokio::test]
async fn managed_download_is_denied_and_returns_without_page_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut socket = accept_async(stream).await.expect("websocket");
        let policy = command(&mut socket).await;
        assert_eq!(policy["method"], "Browser.setDownloadBehavior");
        assert_eq!(
            policy["params"],
            json!({"behavior":"deny","eventsEnabled":false})
        );
        assert!(policy.get("sessionId").is_none());
        socket
            .send(Message::Text(
                json!({"id":policy["id"],"result":{}}).to_string().into(),
            ))
            .await
            .expect("policy response");
        let enable = command(&mut socket).await;
        response(&mut socket, &enable, json!({})).await;
        let navigate = command(&mut socket).await;
        response(
            &mut socket,
            &navigate,
            json!({"frameId":"frame-main","loaderId":"loader-download","isDownload":true,"errorText":"net::ERR_ABORTED"}),
        )
        .await;
    });
    let config = CdpAdapterConfig::new(
        format!("ws://{address}/devtools/browser/capability"),
        CdpLimits::new(64 * 1_024, 1, 16, 2_000).expect("limits"),
    )
    .expect("config");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("transport"));
    client.deny_managed_downloads().await.expect("deny policy");
    let result = client
        .navigate(
            "session-1",
            "https://initial.test:443/archive.bin",
            CdpWaitUntil::Load,
        )
        .await
        .expect("download outcome");
    assert_eq!(result.frame_id, "frame-main");
    assert_eq!(result.loader_id.as_deref(), Some("loader-download"));
    assert_eq!(result.outcome, CdpNavigationOutcome::Download);
    server.await.expect("server");
}
