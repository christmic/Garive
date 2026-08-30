//! Outer-process release gate for sustained Runtime event and reconnect churn.

use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;

const RELEASE_SECONDS: u64 = 30 * 60;
const MAX_RSS_KIB: u64 = 100 * 1024;
const MAX_WINDOW_GROWTH_KIB: u64 = 20 * 1024;

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    build_profile: &'static str,
    duration_ms: u64,
    reconnect_count: u64,
    committed_turn_count: u64,
    rss: RssReport,
    environment: Environment,
}

#[derive(Serialize)]
struct RssReport {
    baseline_kib: u64,
    early_window_peak_kib: u64,
    late_window_peak_kib: u64,
    peak_kib: u64,
    ending_kib: u64,
    absolute_gate_kib: u64,
    window_growth_gate_kib: u64,
}

#[derive(Serialize)]
struct Environment {
    os: String,
    cpu: String,
    rustc: String,
    garive_commit: String,
    terminal_backend: &'static str,
    host_backend: &'static str,
}

fn main() {
    let seconds = parse_seconds();
    let release_gate = seconds == RELEASE_SECONDS;
    let tui = release_path("garive-tui", true);
    assert!(tui.is_file(), "build the release garive-tui binary first");
    let temporary = tempfile::tempdir().unwrap();
    let (address, mut server) = start_runtime_host(temporary.path());
    let sample = measure_churn(&tui, address, temporary.path(), seconds);
    server.kill().unwrap();
    server.wait().unwrap();

    if release_gate {
        assert!(sample.duration_ms >= RELEASE_SECONDS * 1_000);
        assert!(sample.reconnect_count >= 1_000);
        assert!(sample.committed_turn_count >= 100);
        assert!(sample.peak_kib < MAX_RSS_KIB, "peak RSS exceeded 100 MiB");
        assert!(
            sample.late_window_peak_kib
                <= sample
                    .early_window_peak_kib
                    .saturating_add(MAX_WINDOW_GROWTH_KIB),
            "late-window RSS grew by more than 20 MiB"
        );
    }

    let report = Report {
        schema_version: 1,
        status: if release_gate {
            "candidate-release-gate"
        } else {
            "preflight-only"
        },
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        duration_ms: sample.duration_ms,
        reconnect_count: sample.reconnect_count,
        committed_turn_count: sample.committed_turn_count,
        rss: RssReport {
            baseline_kib: sample.baseline_kib,
            early_window_peak_kib: sample.early_window_peak_kib,
            late_window_peak_kib: sample.late_window_peak_kib,
            peak_kib: sample.peak_kib,
            ending_kib: sample.ending_kib,
            absolute_gate_kib: MAX_RSS_KIB,
            window_growth_gate_kib: MAX_WINDOW_GROWTH_KIB,
        },
        environment: Environment {
            os: output("uname", &["-srvmp"]),
            cpu: output("sysctl", &["-n", "machdep.cpu.brand_string"]),
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
        },
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

struct Sample {
    duration_ms: u64,
    reconnect_count: u64,
    committed_turn_count: u64,
    baseline_kib: u64,
    early_window_peak_kib: u64,
    late_window_peak_kib: u64,
    peak_kib: u64,
    ending_kib: u64,
}

fn measure_churn(tui: &Path, host: SocketAddr, root: &Path, seconds: u64) -> Sample {
    let result = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", tui)
        .env("GARIVE_TUI_HOST", format!("http://{host}/"))
        .env("GARIVE_TUI_STATE", root.join("state"))
        .env("GARIVE_CHURN_SECONDS", seconds.to_string())
        .args([
            "-c",
            r#"
                set timeout 10
                encoding system utf-8
                spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --reduced-motion}
                fconfigure $spawn_id -encoding utf-8
                expect -exact "\033\[6n"
                send "\033\[1;1R"
                expect -exact "A quiet place"
                send "churn seed\r"
                expect -exact "Churn event 0 committed."
                set pid [exp_pid]
                set baseline [string trim [exec ps -o rss= -p $pid]]
                set early_peak $baseline
                set late_peak 0
                set peak $baseline
                set reconnects 0
                set turns 1
                set started [clock milliseconds]
                set duration_ms [expr {$env(GARIVE_CHURN_SECONDS) * 1000}]
                set window_ms [expr {min(300000, max(1000, $duration_ms / 3))}]
                log_user 0
                while {[expr {[clock milliseconds] - $started}] < $duration_ms} {
                    send "/reconnect\r"
                    incr reconnects
                    after 250
                    if {$reconnects % 10 == 0} {
                        send "bounded event $reconnects\r"
                        expect -exact "Churn event $turns committed."
                        incr turns
                    }
                    set elapsed [expr {[clock milliseconds] - $started}]
                    set rss [string trim [exec ps -o rss= -p $pid]]
                    if {$rss > $peak} { set peak $rss }
                    if {$elapsed <= $window_ms && $rss > $early_peak} { set early_peak $rss }
                    if {$elapsed >= $duration_ms - $window_ms && $rss > $late_peak} { set late_peak $rss }
                }
                set elapsed [expr {[clock milliseconds] - $started}]
                set ending [string trim [exec ps -o rss= -p $pid]]
                send "\021"
                send "\r"
                expect eof
                puts "CHURN_DURATION_MS=$elapsed"
                puts "CHURN_RECONNECTS=$reconnects"
                puts "CHURN_TURNS=$turns"
                puts "CHURN_BASELINE_KIB=$baseline"
                puts "CHURN_EARLY_PEAK_KIB=$early_peak"
                puts "CHURN_LATE_PEAK_KIB=$late_peak"
                puts "CHURN_PEAK_KIB=$peak"
                puts "CHURN_ENDING_KIB=$ending"
            "#,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "churn PTY measurement failed: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    let output = String::from_utf8_lossy(&result.stdout);
    Sample {
        duration_ms: marker(&output, "CHURN_DURATION_MS="),
        reconnect_count: marker(&output, "CHURN_RECONNECTS="),
        committed_turn_count: marker(&output, "CHURN_TURNS="),
        baseline_kib: marker(&output, "CHURN_BASELINE_KIB="),
        early_window_peak_kib: marker(&output, "CHURN_EARLY_PEAK_KIB="),
        late_window_peak_kib: marker(&output, "CHURN_LATE_PEAK_KIB="),
        peak_kib: marker(&output, "CHURN_PEAK_KIB="),
        ending_kib: marker(&output, "CHURN_ENDING_KIB="),
    }
}

fn marker(output: &str, name: &str) -> u64 {
    output
        .lines()
        .find_map(|line| line.find(name).map(|index| &line[index + name.len()..]))
        .and_then(|value| value.trim_end_matches('\r').trim().parse().ok())
        .unwrap_or_else(|| panic!("missing measurement marker {name}"))
}

fn parse_seconds() -> u64 {
    let mut arguments = std::env::args().skip(1);
    let seconds = match arguments.next().as_deref() {
        None => RELEASE_SECONDS,
        Some("--seconds") => arguments
            .next()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .expect("--seconds requires a positive integer"),
        Some(_) => panic!("usage: release_churn_baseline [--seconds N]"),
    };
    assert!(arguments.next().is_none(), "unexpected argument");
    seconds
}

fn release_path(name: &str, binary: bool) -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(if binary {
            format!("../{name}")
        } else {
            name.into()
        })
}

fn start_runtime_host(root: &Path) -> (SocketAddr, Child) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let host = release_path("visual_demo_host", false);
    assert!(host.is_file(), "build the release visual_demo_host first");
    let child = Command::new(host)
        .arg(root.join("runtime.sqlite"))
        .arg(address.to_string())
        .arg("--churn")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(address).is_err() {
        assert!(
            Instant::now() < deadline,
            "Runtime Host did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    (address, child)
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
