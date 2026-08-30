//! Repeated outer-process release baseline for the first interactive TUI frame.

use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
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
    let temporary = tempfile::tempdir().unwrap();
    let (host, mut server) = start_runtime_host(temporary.path());
    let runs = (0..RUNS)
        .map(|run| measure_run(&tui, host, temporary.path(), run))
        .collect::<Vec<_>>();
    server.kill().unwrap();
    server.wait().unwrap();
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
            host_backend: "production LiveHost with file SQLite",
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
    release_example_path("garive-tui", false)
}

fn release_example_path(name: &str, example: bool) -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(if example { name } else { "../garive-tui" })
}

fn start_runtime_host(root: &Path) -> (SocketAddr, Child) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let host = release_example_path("visual_demo_host", true);
    assert!(host.is_file(), "build the release visual_demo_host first");
    let child = Command::new(host)
        .arg(root.join("runtime.sqlite"))
        .arg(address.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(address).is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Runtime Host did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    (address, child)
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
