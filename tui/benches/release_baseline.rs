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

use std::{hint::black_box, process::Command, time::Instant};

use application::{AppModel, BootState, TimelineItem, TimelineRole, TurnBlock, TurnBlockKey};
use garive_host_client::{reduce_host_events, HostActivity, HostEvent, HostView};
use ratatui::{buffer::Buffer, layout::Rect};
use serde::Serialize;
use sha2::{Digest, Sha256};

const RUNS: usize = 3;
const WARMUP: usize = 20;
const SAMPLES: usize = 200;

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    build_profile: &'static str,
    fixture_sha256: String,
    warmup_samples: usize,
    measured_samples_per_run: usize,
    environment: Environment,
    not_measured: [&'static str; 3],
    runs: Vec<Run>,
}

#[derive(Serialize)]
struct Environment {
    os: String,
    cpu: String,
    rustc: String,
    garive_commit: String,
    terminal_backend: &'static str,
}

#[derive(Serialize)]
struct Run {
    key_to_model_us: Distribution,
    render_120x40_200_cells_us: Distribution,
    resize_200x60_1000_cells_us: Distribution,
    h3_event_reduction_per_second: Throughput,
    unloaded_page_growth_ratio_milli: u64,
}

#[derive(Serialize)]
struct Distribution {
    p50: u64,
    p95: u64,
    p99: u64,
    max: u64,
}

#[derive(Serialize)]
struct Throughput {
    p05: u64,
    p50: u64,
    p95: u64,
    min: u64,
}

fn main() {
    let corpus = timeline(10_000);
    let h3_events = h3_events();
    let mut fixture = fixture_bytes(&corpus);
    fixture.extend(serde_jcs::to_vec(&h3_events).unwrap());
    let fixture_sha256 = format!("{:x}", Sha256::digest(fixture));
    let runs = (0..RUNS)
        .map(|_| measure_run(&corpus, &h3_events))
        .collect::<Vec<_>>();
    for run in &runs {
        assert!(run.key_to_model_us.p99 < 2_000);
        assert!(run.render_120x40_200_cells_us.p99 < 16_000);
        assert!(run.resize_200x60_1000_cells_us.p95 < 33_000);
        assert!(run.h3_event_reduction_per_second.p05 > 100_000);
        assert!(run.unloaded_page_growth_ratio_milli < 2_000);
    }
    let report = Report {
        schema_version: 1,
        status: "candidate-in-process",
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "bench"
        },
        fixture_sha256,
        warmup_samples: WARMUP,
        measured_samples_per_run: SAMPLES,
        environment: environment(),
        not_measured: [
            "first interactive PTY frame",
            "60-second idle CPU",
            "peak resident memory",
        ],
        runs,
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn environment() -> Environment {
    Environment {
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
        terminal_backend: "in-process ratatui Buffer",
    }
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

fn measure_run(corpus: &[TimelineItem], h3_events: &[HostEvent]) -> Run {
    let key_to_model_us = measure_editor();
    let render_120x40_200_cells_us = measure_render(&corpus[..200], Rect::new(0, 0, 120, 40));
    let resize_200x60_1000_cells_us = measure_resize(&corpus[..1_000]);
    let h3_event_reduction_per_second = measure_h3(h3_events);
    let bounded = median_render_ns(&corpus[..200]);
    let unloaded = median_render_ns(corpus);
    Run {
        key_to_model_us,
        render_120x40_200_cells_us,
        resize_200x60_1000_cells_us,
        h3_event_reduction_per_second,
        unloaded_page_growth_ratio_milli: unloaded.saturating_mul(1_000) / bounded.max(1),
    }
}

fn measure_h3(events: &[HostEvent]) -> Throughput {
    let mut samples = measure(|| {
        black_box(reduce_host_events(
            "session-benchmark",
            events,
            HostView::default(),
            events.len(),
        ))
        .unwrap();
    })
    .into_iter()
    .map(|nanoseconds| events.len() as u64 * 1_000_000_000 / nanoseconds.max(1))
    .collect::<Vec<_>>();
    samples.sort_unstable();
    Throughput {
        p05: percentile(&samples, 5),
        p50: percentile(&samples, 50),
        p95: percentile(&samples, 95),
        min: samples[0],
    }
}

fn measure_editor() -> Distribution {
    let seed = "界a".repeat(1_023) + "界";
    assert_eq!(seed.len(), 4_095);
    let mut editor = input::EditorState::new(8_192);
    editor.insert(&seed).unwrap();
    let samples = measure(|| {
        editor.insert("界").unwrap();
        black_box(editor.text());
        assert!(editor.backspace());
    });
    distribution(samples, 1_000)
}

fn measure_render(items: &[TimelineItem], area: Rect) -> Distribution {
    let model = model(items);
    let mut cache = view::RenderCache::default();
    let samples = measure(|| {
        let mut buffer = Buffer::empty(area);
        black_box(view::render_cached(
            &model,
            Theme::Dark,
            area,
            &mut buffer,
            &mut cache,
        ));
    });
    distribution(samples, 1_000)
}

fn measure_resize(items: &[TimelineItem]) -> Distribution {
    let model = model(items);
    let small = Rect::new(0, 0, 120, 40);
    let large = Rect::new(0, 0, 200, 60);
    let resize_once = || {
        let mut cache = view::RenderCache::default();
        let mut small_buffer = Buffer::empty(small);
        black_box(view::render_cached(
            &model,
            Theme::Dark,
            small,
            &mut small_buffer,
            &mut cache,
        ));
        let started = Instant::now();
        let mut large_buffer = Buffer::empty(large);
        black_box(view::render_cached(
            &model,
            Theme::Dark,
            large,
            &mut large_buffer,
            &mut cache,
        ));
        started.elapsed().as_nanos() as u64
    };
    for _ in 0..WARMUP {
        black_box(resize_once());
    }
    let samples = (0..SAMPLES).map(|_| resize_once()).collect();
    distribution(samples, 1_000)
}

fn median_render_ns(items: &[TimelineItem]) -> u64 {
    let model = model(items);
    let area = Rect::new(0, 0, 120, 40);
    let mut cache = view::RenderCache::default();
    let mut samples = (0..50)
        .map(|_| {
            let mut buffer = Buffer::empty(area);
            let started = Instant::now();
            black_box(view::render_cached(
                &model,
                Theme::Dark,
                area,
                &mut buffer,
                &mut cache,
            ));
            started.elapsed().as_nanos() as u64
        })
        .collect::<Vec<_>>();
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn measure(mut operation: impl FnMut()) -> Vec<u64> {
    for _ in 0..WARMUP {
        operation();
    }
    (0..SAMPLES)
        .map(|_| {
            let started = Instant::now();
            operation();
            started.elapsed().as_nanos() as u64
        })
        .collect()
}

fn distribution(mut samples: Vec<u64>, divisor: u64) -> Distribution {
    samples.sort_unstable();
    Distribution {
        p50: percentile(&samples, 50) / divisor,
        p95: percentile(&samples, 95) / divisor,
        p99: percentile(&samples, 99) / divisor,
        max: *samples.last().unwrap() / divisor,
    }
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    samples[(samples.len() - 1) * percentile / 100]
}

fn model(items: &[TimelineItem]) -> AppModel {
    AppModel {
        boot: BootState::Ready,
        turn_blocks: items
            .chunks(3)
            .enumerate()
            .map(|(index, children)| TurnBlock {
                key: TurnBlockKey {
                    session_id: "session-benchmark".into(),
                    turn_id: format!("turn-{index}"),
                },
                user: children[0].clone(),
                activities: children.get(1).cloned().into_iter().collect(),
                committed_answer: children.get(2).cloned(),
                outcome: None,
            })
            .collect(),
        ..Default::default()
    }
}

fn timeline(count: usize) -> Vec<TimelineItem> {
    (1..=count)
        .map(|position| TimelineItem {
            stable_key: format!("item-{position}"),
            position: position as u64,
            role: match position % 3 {
                0 => TimelineRole::Agent,
                1 => TimelineRole::User,
                _ => TimelineRole::Status,
            },
            tone: Default::default(),
            text: format!(
                "Bounded cell {position}: Unicode 界, emoji 🦀, and **safe Markdown** remain visible."
            ),
        })
        .collect()
}

fn h3_events() -> Vec<HostEvent> {
    (1..=10_000)
        .map(|position| HostEvent {
            api_version: "v1".into(),
            session_id: "session-benchmark".into(),
            position,
            event: "agent.activity.prepared".into(),
            turn_id: "turn-benchmark".into(),
            execution_id: "execution-benchmark".into(),
            text: String::new(),
            activity: Some(HostActivity {
                api_version: "v1".into(),
                activity_id: format!("activity-{position}"),
                kind: "tool".into(),
                label_key: "activity.tool".into(),
                state: "prepared".into(),
                source_position: position,
                terminal: false,
                safe_code: None,
            }),
        })
        .collect()
}

fn fixture_bytes(items: &[TimelineItem]) -> Vec<u8> {
    items
        .iter()
        .flat_map(|item| {
            format!(
                "{}\t{}\t{:?}\t{}\n",
                item.stable_key, item.position, item.role, item.text
            )
            .into_bytes()
        })
        .collect()
}
