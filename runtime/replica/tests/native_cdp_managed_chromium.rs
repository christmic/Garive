#![cfg(target_os = "macos")]

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use garive_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport, CdpWaitUntil};
use garive_runtime::{
    BrowserPageId, BrowserSessionId, CdpNativeAdapterPort, NativeActionCommandV1, NativeActionId,
    NativeAdapterPort, NativeObservationBounds, NativeTarget,
};
use serde_json::json;

const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

struct ManagedBrowser(Child);

impl Drop for ManagedBrowser {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct LocalPageServer {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalPageServer {
    fn start(cross_origin: Option<SocketAddr>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fixture listener");
        listener.set_nonblocking(true).expect("nonblocking fixture");
        let address = listener.local_addr().expect("fixture address");
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        serve(&mut stream, address, cross_origin);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            stop,
            worker: Some(worker),
        }
    }

    fn start_url(&self) -> String {
        format!("http://{}/start", self.address)
    }

    fn seed_url(&self) -> String {
        format!("http://{}/seed", self.address)
    }

    fn origin(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for LocalPageServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(stream: &mut TcpStream, address: SocketAddr, cross_origin: Option<SocketAddr>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let mut request = [0_u8; 8_192];
    let size = stream.read(&mut request).unwrap_or(0);
    let request = String::from_utf8_lossy(&request[..size]);
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let (status, headers, body) = match path {
        "/start" => (
            "302 Found",
            format!("Location: http://{address}/form\r\n"),
            String::new(),
        ),
        "/form" => (
            "200 OK",
            "Content-Type: text/html; charset=utf-8\r\n".into(),
            format!(
                r#"<!doctype html><title>Runtime port fixture</title><body style="height:4000px" onscroll="document.querySelector('main').setAttribute('aria-label','Scrolled')"><main><label>Account <input aria-label="Account name"></label><label>Vault <input type="password" aria-label="Vault password" value="GARIVE_PASSWORD_CANARY_DO_NOT_LEAK"></label><label>Mode <select aria-label="Execution mode" onchange="this.setAttribute('aria-label','Execution mode '+this.value)"><option value="safe">Safe</option><option value="stable">Stable</option></select></label><button data-count="0" onclick="this.dataset.count=String(Number(this.dataset.count)+1);this.setAttribute('aria-label','Submitted '+this.dataset.count)">Submit form</button><iframe src="/same-frame"></iframe><iframe src="http://{}/cross-frame"></iframe></main></body>"#,
                cross_origin.expect("cross-origin fixture")
            ),
        ),
        "/same-frame" => (
            "200 OK",
            "Content-Type: text/html; charset=utf-8\r\n".into(),
            "<button aria-label=\"Same frame action\">Same</button>".into(),
        ),
        "/seed" => (
            "200 OK",
            "Content-Type: text/html; charset=utf-8\r\n".into(),
            "<!doctype html><title>Managed seed</title>".into(),
        ),
        "/cross-frame" => (
            "200 OK",
            "Content-Type: text/html; charset=utf-8\r\n".into(),
            "<button aria-label=\"Cross origin secret\">Secret</button>".into(),
        ),
        _ => ("204 No Content", String::new(), String::new()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn launch_chrome(profile: &std::path::Path) -> ManagedBrowser {
    let child = Command::new(CHROME)
        .args([
            "--headless=new",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-default-apps",
            "--disable-extensions",
            "--disable-sync",
            "--metrics-recording-only",
            "--no-first-run",
            "--no-default-browser-check",
            "--remote-debugging-port=0",
            &format!("--user-data-dir={}", profile.display()),
            "about:blank",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch managed Chrome");
    ManagedBrowser(child)
}

fn endpoint(profile: &std::path::Path) -> String {
    let active_port = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !active_port.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    let active = fs::read_to_string(active_port).expect("DevToolsActivePort");
    let mut lines = active.lines();
    let port = lines.next().expect("port");
    let path = lines.next().expect("browser capability path");
    format!("ws://127.0.0.1:{port}{path}")
}

fn target(page_id: &str) -> NativeTarget {
    NativeTarget::Browser {
        session_id: BrowserSessionId::new("managed-chrome").expect("browser session"),
        page_id: BrowserPageId::new(page_id).expect("page"),
    }
}

#[tokio::test]
#[ignore = "requires installed local Google Chrome; run explicitly for native evidence"]
async fn managed_chrome_runs_through_the_governed_runtime_port() {
    assert!(
        std::path::Path::new(CHROME).is_file(),
        "Chrome is not installed"
    );
    let profile = tempfile::tempdir().expect("temporary managed profile");
    let cross_origin_server = LocalPageServer::start(None);
    let page_server = LocalPageServer::start(Some(cross_origin_server.address));
    let _browser = launch_chrome(profile.path());
    let config = CdpAdapterConfig::new(
        endpoint(profile.path()),
        CdpLimits::new(4 * 1_024 * 1_024, 1, 1_024, 10_000).expect("limits"),
    )
    .expect("explicit endpoint");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("connect"));
    let page_id = client.create_blank_target().await.expect("target");
    let cdp_session_id = client.attach_target(&page_id).await.expect("attach");
    client
        .navigate(&cdp_session_id, &page_server.seed_url(), CdpWaitUntil::Load)
        .await
        .expect("managed HTTP seed");
    let target = target(&page_id);
    let mut port = CdpNativeAdapterPort::new(
        target.clone(),
        cdp_session_id,
        "revision-initial",
        "managed-runtime-port",
        64,
        client,
    )
    .expect("runtime port");
    let bounds = NativeObservationBounds {
        max_nodes: 1_024,
        max_text_bytes: 1_048_576,
    };
    let before = port
        .observe(&target, None, bounds)
        .await
        .expect("initial observation");
    let navigation = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-navigation").expect("action"),
        target: target.clone(),
        expected_snapshot_id: before.snapshot_id.clone(),
        target_revision: before.target_revision.clone(),
        prepared_input: json!({
            "destination_url":page_server.start_url(),
            "destination_origin":page_server.origin(),
            "wait_until":"load",
            "timeout_ms":10_000,
            "max_nodes":bounds.max_nodes,
            "max_text_bytes":bounds.max_text_bytes
        }),
    };
    let binding = port.preflight_action(&navigation).expect("preflight");
    let receipt = port
        .dispatch_action(&navigation, &binding)
        .await
        .expect("navigation receipt");
    assert_eq!(receipt.terminal_classification, "completed");
    assert!(receipt.failure_code.is_none());

    let after = port
        .observe(&target, Some(&before.snapshot_id), bounds)
        .await
        .expect("post-navigation observation");
    assert_ne!(after.target_revision, before.target_revision);
    assert!(after
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Submit form")));
    assert!(after
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Account name")));
    assert!(
        after
            .nodes
            .iter()
            .any(|node| node.name.as_deref() == Some("Same frame action")),
        "{:#?}",
        after.nodes
    );
    assert!(!after
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Cross origin secret")));
    assert!(!after.nodes.iter().any(|node| {
        node.name.as_deref() == Some("GARIVE_PASSWORD_CANARY_DO_NOT_LEAK")
            || node.value_summary.as_deref() == Some("GARIVE_PASSWORD_CANARY_DO_NOT_LEAK")
    }));
    let password = after
        .nodes
        .iter()
        .find(|node| {
            node.role == "textbox"
                && node.sensitivity == garive_runtime::NativeSensitivity::Redacted
        })
        .expect("redacted password field");
    assert_eq!(password.name.as_deref(), Some("[redacted]"));
    assert_eq!(password.value_summary.as_deref(), Some("[redacted]"));
    assert!(password.actions.is_empty());
    let opaque_frame = after
        .nodes
        .iter()
        .find(|node| node.role == "opaque_frame")
        .expect("cross-origin opaque frame");
    assert!(opaque_frame.name.is_none());
    let opaque_click = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-opaque-frame").expect("action"),
        target: target.clone(),
        expected_snapshot_id: after.snapshot_id.clone(),
        target_revision: after.target_revision.clone(),
        prepared_input: json!({
            "action":"click",
            "node_ref":opaque_frame.node_ref.as_str(),
            "allowed_navigation_origins":[]
        }),
    };
    assert_eq!(
        port.preflight_action(&opaque_click),
        Err(garive_runtime::NativeProtocolError::BrowserFrameOpaque)
    );

    let mode = after
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("Execution mode"))
        .expect("mode select")
        .node_ref
        .clone();
    let select = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-select").expect("action"),
        target: target.clone(),
        expected_snapshot_id: after.snapshot_id.clone(),
        target_revision: after.target_revision.clone(),
        prepared_input: json!({
            "action":"select_option",
            "node_ref":mode.as_str(),
            "option":"stable",
            "allowed_navigation_origins":[]
        }),
    };
    let select_binding = port.preflight_action(&select).expect("select preflight");
    let select_receipt = port
        .dispatch_action(&select, &select_binding)
        .await
        .expect("select receipt");
    assert_eq!(select_receipt.terminal_classification, "completed");
    let after_select = port
        .observe(&target, Some(&after.snapshot_id), bounds)
        .await
        .expect("post-select observation");
    assert!(after_select
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Execution mode stable")));

    let submit = after_select
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("Submit form"))
        .expect("submit")
        .node_ref
        .clone();
    let click = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-click").expect("action"),
        target: target.clone(),
        expected_snapshot_id: after_select.snapshot_id.clone(),
        target_revision: after_select.target_revision.clone(),
        prepared_input: json!({
            "action":"click",
            "node_ref":submit.as_str(),
            "allowed_navigation_origins":[]
        }),
    };
    let click_binding = port.preflight_action(&click).expect("click preflight");
    port.dispatch_action(&click, &click_binding)
        .await
        .expect("click receipt");
    let after_click = port
        .observe(&target, Some(&after_select.snapshot_id), bounds)
        .await
        .expect("post-click observation");
    let submitted = after_click
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("Submitted 1"))
        .expect("submitted button");
    assert_eq!(after_click.focused_node.as_ref(), Some(&submitted.node_ref));

    let key = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-key").expect("action"),
        target: target.clone(),
        expected_snapshot_id: after_click.snapshot_id.clone(),
        target_revision: after_click.target_revision.clone(),
        prepared_input: json!({
            "action":"press_key",
            "key":"enter",
            "allowed_navigation_origins":[]
        }),
    };
    let key_binding = port.preflight_action(&key).expect("key preflight");
    port.dispatch_action(&key, &key_binding)
        .await
        .expect("key receipt");
    let after_key = port
        .observe(&target, Some(&after_click.snapshot_id), bounds)
        .await
        .expect("post-key observation");
    assert!(after_key
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Submitted 2")));

    let scroll = NativeActionCommandV1 {
        action_id: NativeActionId::new("managed-scroll").expect("action"),
        target: target.clone(),
        expected_snapshot_id: after_key.snapshot_id.clone(),
        target_revision: after_key.target_revision.clone(),
        prepared_input: json!({
            "action":"scroll",
            "delta_x":0,
            "delta_y":600,
            "allowed_navigation_origins":[]
        }),
    };
    let scroll_binding = port.preflight_action(&scroll).expect("scroll preflight");
    port.dispatch_action(&scroll, &scroll_binding)
        .await
        .expect("scroll receipt");
    let after_scroll = port
        .observe(&target, Some(&after_key.snapshot_id), bounds)
        .await
        .expect("post-scroll observation");
    assert!(after_scroll
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Scrolled")));
}
