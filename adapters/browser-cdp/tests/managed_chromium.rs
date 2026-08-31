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

use garive_adapter_browser_cdp::{
    CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport, CdpWaitUntil,
};

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
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
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

    fn final_url(&self) -> String {
        format!("http://{}/form", self.address)
    }

    fn popup_url(&self) -> String {
        format!("http://{}/popup", self.address)
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
            r#"<!doctype html><title>Garive native fixture</title><main><label>Account <input aria-label="Account name"></label><button onclick="this.setAttribute('aria-label','Submitted')">Submit form</button><button aria-label="Open popup" onclick="window.open('/popup','garive-child')">Open popup</button><div id="shadow"></div></main><script>document.querySelector('#shadow').attachShadow({mode:'open'}).innerHTML='<button>Shadow action</button>';</script>"#,
        ),
        "/popup" => (
            "200 OK",
            "Content-Type: text/html; charset=utf-8\r\n".into(),
            "<!doctype html><title>Popup fixture</title><main aria-label=\"Popup ready\"></main>",
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

#[tokio::test]
#[ignore = "requires an installed local Google Chrome; run explicitly for native evidence"]
async fn managed_chrome_version_target_attach_and_ax_tree() {
    assert!(
        std::path::Path::new(CHROME).is_file(),
        "Chrome is not installed"
    );
    let profile = tempfile::tempdir().expect("temporary managed profile");
    let page_server = LocalPageServer::start();
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
            &format!("--user-data-dir={}", profile.path().display()),
            "about:blank",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch managed Chrome");
    let _browser = ManagedBrowser(child);
    let active_port = profile.path().join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !active_port.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    let active = fs::read_to_string(active_port).expect("DevToolsActivePort");
    let mut lines = active.lines();
    let port = lines.next().expect("port");
    let path = lines.next().expect("browser capability path");
    let config = CdpAdapterConfig::new(
        format!("ws://127.0.0.1:{port}{path}"),
        CdpLimits::new(4 * 1_024 * 1_024, 1, 1_024, 10_000).expect("limits"),
    )
    .expect("explicit endpoint");
    let mut client = CdpClient::new(CdpTransport::connect(&config).await.expect("connect"));
    let version = client.browser_version().await.expect("version");
    assert!(version.product.starts_with("Chrome/"));
    let target = client.create_blank_target().await.expect("target");
    let session = client.attach_target(&target).await.expect("attach");
    client
        .enable_accessibility(&session)
        .await
        .expect("enable accessibility");
    let navigation = client
        .navigate(&session, &page_server.start_url(), CdpWaitUntil::Load)
        .await
        .expect("redirected navigation");
    assert_eq!(navigation.final_url, page_server.final_url());
    let tree = client
        .full_ax_tree(&session, None, 64, 10_000, 1_048_576)
        .await
        .expect("AX tree");
    assert!(!tree.nodes.is_empty());
    assert!(tree
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Submit form")));
    assert!(tree
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Account name")));
    assert!(tree
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Shadow action")));
    let submit = tree
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("Submit form"))
        .and_then(|node| node.backend_dom_node_id)
        .expect("submit backend node");
    let account = tree
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("Account name"))
        .and_then(|node| node.backend_dom_node_id)
        .expect("account backend node");
    let popup_button = tree
        .nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("Open popup"))
        .and_then(|node| node.backend_dom_node_id)
        .expect("popup backend node");
    client
        .click_backend_node(&session, submit)
        .await
        .expect("semantic click");
    let after_click = client
        .full_ax_tree(&session, None, 64, 10_000, 1_048_576)
        .await
        .expect("post-click AX tree");
    assert!(after_click
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Submitted")));
    client
        .type_text_backend_node(&session, account, "Garive 🦀")
        .await
        .expect("type text");
    let after_type = client
        .full_ax_tree(&session, None, 64, 10_000, 1_048_576)
        .await
        .expect("post-type AX tree");
    assert!(after_type
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Account name")
            && node.value_summary.as_deref() == Some("Garive 🦀")));
    client
        .clear_backend_node(&session, account)
        .await
        .expect("clear text");
    let after_clear = client
        .full_ax_tree(&session, None, 64, 10_000, 1_048_576)
        .await
        .expect("post-clear AX tree");
    assert!(after_clear
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Account name") && node.value_summary.is_none()));
    client
        .enable_managed_popup_tracking(&session)
        .await
        .expect("popup tracking");
    client.begin_popup_action(&session).expect("popup action");
    client
        .click_backend_node(&session, popup_button)
        .await
        .expect("popup click");
    client.frame_tree(&session).await.expect("popup event pump");
    let popup = client
        .take_popup(&session, &target)
        .await
        .expect("popup collection")
        .expect("popup target");
    assert_eq!(popup.opener_id, target);
    assert_eq!(popup.requested_url, page_server.popup_url());
    assert!(popup.user_gesture);
    let popup_session = client
        .attach_target(&popup.target_id)
        .await
        .expect("explicit popup admission");
    client
        .enable_accessibility(&popup_session)
        .await
        .expect("popup accessibility");
    let popup_tree = client
        .full_ax_tree(&popup_session, None, 64, 10_000, 1_048_576)
        .await
        .expect("popup AX tree");
    assert!(popup_tree
        .nodes
        .iter()
        .any(|node| node.name.as_deref() == Some("Popup ready")));
}
