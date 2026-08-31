//! Isolated release-process peak-RSS gate for the bounded production TUI model.

#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::Theme;
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;
#[path = "../src/view/mod.rs"]
mod view;

use std::{hint::black_box, process::Command};

use application::{AppModel, BootState, TimelineItem, TimelineRole, TurnBlock, TurnBlockKey};
use garive_host_client::SessionSummary;
use ratatui::{buffer::Buffer, layout::Rect};
use serde::Serialize;

const RUNS: usize = 3;
const SESSIONS: usize = 10;
const CELLS: usize = 5_000;
const LIMIT_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    build_profile: &'static str,
    workload: Workload,
    environment: Environment,
    peak_rss_bytes: Vec<u64>,
    gate_bytes: u64,
}

#[derive(Serialize)]
struct Workload {
    sessions: usize,
    loaded_timeline_cells: usize,
    render_area: &'static str,
}

#[derive(Serialize)]
struct Environment {
    os: String,
    cpu: String,
    rustc: String,
    garive_commit: String,
    measurement_backend: &'static str,
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--sample") {
        run_sample();
        return;
    }
    let executable = std::env::current_exe().unwrap();
    let peaks = (0..RUNS)
        .map(|_| measure_peak(&executable))
        .collect::<Vec<_>>();
    assert!(
        peaks.iter().all(|peak| *peak < LIMIT_BYTES),
        "peak RSS exceeded 100 MiB"
    );
    let report = Report {
        schema_version: 1,
        status: "candidate-isolated-process",
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        workload: Workload {
            sessions: SESSIONS,
            loaded_timeline_cells: CELLS,
            render_area: "200x60",
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
            measurement_backend: "Darwin /usr/bin/time -l child peak RSS",
        },
        peak_rss_bytes: peaks,
        gate_bytes: LIMIT_BYTES,
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn run_sample() {
    let cells = timeline();
    let mut model = AppModel {
        boot: BootState::Ready,
        session_count: SESSIONS,
        sessions: sessions(),
        selected_session: Some("session-0".into()),
        turn_blocks: cells
            .chunks(3)
            .enumerate()
            .map(|(index, children)| TurnBlock {
                key: TurnBlockKey {
                    session_id: "session-0".into(),
                    turn_id: format!("turn-{index}"),
                },
                user: children[0].clone(),
                activities: children.get(1).cloned().into_iter().collect(),
                committed_answer: children.get(2).cloned(),
                outcome: None,
            })
            .collect(),
        ..Default::default()
    };
    model.follow_latest();
    let area = Rect::new(0, 0, 200, 60);
    let mut buffer = Buffer::empty(area);
    let mut cache = view::RenderCache::default();
    view::render_cached(&model, Theme::Dark, area, &mut buffer, &mut cache);
    black_box((&model, &buffer, &cache));
    assert_eq!(model.sessions.len(), SESSIONS);
    assert_eq!(model.durable_children().count(), CELLS);
}

fn measure_peak(executable: &std::path::Path) -> u64 {
    let result = Command::new("/usr/bin/time")
        .args(["-l"])
        .arg(executable)
        .arg("--sample")
        .output()
        .unwrap();
    assert!(result.status.success());
    String::from_utf8_lossy(&result.stderr)
        .lines()
        .find(|line| line.contains("maximum resident set size"))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("time -l must report maximum resident set size")
}

fn sessions() -> Vec<SessionSummary> {
    (0..SESSIONS)
        .map(|index| SessionSummary {
            api_version: "v1".into(),
            session_id: format!("session-{index}"),
            agent_instance_id: format!("agent-{index}"),
            definition_id: "benchmark-agent".into(),
            definition_revision: "revision-1".into(),
            opened_at: "2026-08-30T00:00:00Z".into(),
            latest_position: CELLS as u64,
            latest_turn_id: Some(format!("turn-{index}")),
            latest_turn_state: Some("completed".into()),
            turn_count: 250,
        })
        .collect()
}

fn timeline() -> Vec<TimelineItem> {
    (1..=CELLS)
        .map(|position| TimelineItem {
            stable_key: format!("cell-{position}"),
            position: position as u64,
            role: match position % 3 {
                0 => TimelineRole::Agent,
                1 => TimelineRole::User,
                _ => TimelineRole::Status,
            },
            tone: Default::default(),
            text: format!("Bounded cell {position}: Unicode 界, emoji 🦀, and **safe Markdown**."),
        })
        .collect()
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
