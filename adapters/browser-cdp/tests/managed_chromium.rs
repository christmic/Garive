#![cfg(target_os = "macos")]

use std::{
    fs,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use garive_adapter_browser_cdp::{CdpAdapterConfig, CdpClient, CdpLimits, CdpTransport};

const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

struct ManagedBrowser(Child);

impl Drop for ManagedBrowser {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
#[ignore = "requires an installed local Google Chrome; run explicitly for native evidence"]
async fn managed_chrome_version_target_attach_and_ax_tree() {
    assert!(
        std::path::Path::new(CHROME).is_file(),
        "Chrome is not installed"
    );
    let profile = tempfile::tempdir().expect("temporary managed profile");
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
    let tree = client
        .full_ax_tree(&session, None, 64, 10_000, 1_048_576)
        .await
        .expect("AX tree");
    assert!(!tree.nodes.is_empty());
}
