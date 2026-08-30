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

use garive_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport};
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
    fn start() -> Self {
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
                        serve(&mut stream, address);
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

fn serve(stream: &mut TcpStream, address: SocketAddr) {
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
            "",
        ),
        "/form" => (
            "200 OK",
            "Content-Type: text/html; charset=utf-8\r\n".into(),
            r#"<!doctype html><title>Runtime port fixture</title><main><label>Account <input aria-label="Account name"></label><button onclick="this.setAttribute('aria-label','Submitted')">Submit form</button></main>"#,
        ),
        _ => ("204 No Content", String::new(), ""),
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
    let page_server = LocalPageServer::start();
    let _browser = launch_chrome(profile.path());
    let config = CdpAdapterConfig::new(
        endpoint(profile.path()),
        CdpLimits::new(4 * 1_024 * 1_024, 1, 1_024, 10_000).expect("limits"),
    )
    .expect("explicit endpoint");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("connect"));
    let page_id = client.create_blank_target().await.expect("target");
    let cdp_session_id = client.attach_target(&page_id).await.expect("attach");
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
}
