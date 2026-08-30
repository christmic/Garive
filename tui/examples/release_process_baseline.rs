//! Repeated outer-process release baseline for the first interactive TUI frame.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use serde::Serialize;

const RUNS: usize = 3;
const SAMPLES: usize = 20;

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    build_profile: &'static str,
    metric: &'static str,
    environment: Environment,
    runs: Vec<Distribution>,
}

#[derive(Serialize)]
struct Environment {
    os: String,
    rustc: String,
    garive_commit: String,
    terminal_backend: &'static str,
    host_backend: &'static str,
    samples_per_run: usize,
}

#[derive(Serialize)]
struct Distribution {
    p50_us: u64,
    p95_us: u64,
    p99_us: u64,
    max_us: u64,
}

fn main() {
    let tui = release_tui_path();
    assert!(tui.is_file(), "build the release garive-tui binary first");
    let (host, shutdown, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let runs = (0..RUNS)
        .map(|run| measure_run(&tui, host, temporary.path(), run))
        .collect::<Vec<_>>();
    shutdown.store(true, Ordering::SeqCst);
    server.join().unwrap();
    for run in &runs {
        assert!(
            run.p95_us < 150_000,
            "first interactive frame exceeded 150 ms"
        );
    }
    let report = Report {
        schema_version: 1,
        status: "candidate-outer-process",
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        metric: "process start to first interactive frame",
        environment: Environment {
            os: output("uname", &["-srvmp"]),
            rustc: output("rustc", &["--version"]),
            garive_commit: output(
                "git",
                &[
                    "-C",
                    concat!(env!("CARGO_MANIFEST_DIR"), "/.."),
                    "rev-parse",
                    "HEAD",
                ],
            ),
            terminal_backend: "shipping binary under expect PTY",
            host_backend: "bounded loopback H1 boot fixture",
            samples_per_run: SAMPLES,
        },
        runs,
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn measure_run(tui: &Path, host: SocketAddr, root: &Path, run: usize) -> Distribution {
    let mut samples = (0..SAMPLES)
        .map(|sample| measure_start(tui, host, root, run, sample))
        .collect::<Vec<_>>();
    samples.sort_unstable();
    Distribution {
        p50_us: percentile(&samples, 50),
        p95_us: percentile(&samples, 95),
        p99_us: percentile(&samples, 99),
        max_us: *samples.last().unwrap(),
    }
}

fn measure_start(tui: &Path, host: SocketAddr, root: &Path, run: usize, sample: usize) -> u64 {
    let state = root.join(format!("run-{run}-sample-{sample}"));
    let result = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", tui)
        .env("GARIVE_TUI_HOST", format!("http://{host}/"))
        .env("GARIVE_TUI_STATE", state)
        .args([
            "-c",
            r#"
                set timeout 5
                set started [clock microseconds]
                spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --reduced-motion}
                expect -exact "\033\[6n"
                send "\033\[1;1R"
                expect -exact "GARIVE"
                puts "FRAME_US=[expr {[clock microseconds] - $started}]"
                send "\021"
                send "\r"
                expect {
                    eof {}
                    timeout { exit 24 }
                }
            "#,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "PTY measurement failed: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    String::from_utf8_lossy(&result.stdout)
        .lines()
        .find_map(|line| {
            line.find("FRAME_US=")
                .map(|index| &line[index + "FRAME_US=".len()..])
        })
        .and_then(|value| value.trim_end_matches('\r').parse().ok())
        .expect("expect must emit FRAME_US")
}

fn release_tui_path() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("garive-tui")
}

fn empty_host() -> (SocketAddr, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let stop = shutdown.clone();
    let server = thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket.set_nonblocking(false).unwrap();
                    respond(&mut socket);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("loopback Host failed: {error}"),
            }
        }
    });
    (address, shutdown, server)
}

fn respond(socket: &mut std::net::TcpStream) {
    let mut request = [0; 8_192];
    let read = socket.read(&mut request).unwrap();
    let request = String::from_utf8_lossy(&request[..read]);
    let body = if request.contains("GET /v1/agent-definitions ") {
        r#"{"api_version":"v1","definitions":[{"api_version":"v1","definition_id":"benchmark","definition_revision":"v1","capabilities":[]}]}"#
    } else if request.contains("GET /v1/sessions?") {
        r#"{"api_version":"v1","sessions":[],"next_before":null}"#
    } else {
        return;
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).unwrap();
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    samples[(samples.len() - 1) * percentile / 100]
}

fn output(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|result| result.status.success())
        .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".into())
}
