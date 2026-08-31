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

use std::time::Instant;

use application::{AppModel, BootState, TimelineItem, TimelineRole};
use ratatui::{buffer::Buffer, layout::Rect};

#[test]
fn representative_render_and_editor_latency_stay_interactive() {
    let mut model = AppModel {
        boot: BootState::Ready,
        ..Default::default()
    };
    for position in 1..=10_000 {
        model.push_test_timeline_item(TimelineItem {
            stable_key: format!("item-{position}"),
            position,
            role: if position % 3 == 0 {
                TimelineRole::Agent
            } else if position % 3 == 1 {
                TimelineRole::User
            } else {
                TimelineRole::Status
            },
            tone: Default::default(),
            text: format!(
                "Bounded timeline row {position}: Unicode 界 and **safe Markdown** remain visible."
            ),
        });
    }
    let area = Rect::new(0, 0, 120, 40);
    let mut cache = view::RenderCache::default();
    let mut samples = Vec::new();
    for _ in 0..110 {
        let mut buffer = Buffer::empty(area);
        let started = Instant::now();
        let _ = view::render_cached(&model, Theme::Dark, area, &mut buffer, &mut cache);
        samples.push(started.elapsed().as_micros());
    }
    samples.drain(..10);
    samples.sort_unstable();
    let render_p50 = samples[49];
    let render_p95 = samples[94];

    let mut editor = input::EditorState::new(4_096);
    let mut editor_samples = Vec::new();
    for _ in 0..100 {
        let started = Instant::now();
        editor.insert("界").unwrap();
        editor_samples.push(started.elapsed().as_micros());
    }
    editor_samples.sort_unstable();
    let editor_p95 = editor_samples[94];
    eprintln!(
        "TUI_BASELINE render_p50_us={render_p50} render_p95_us={render_p95} editor_p95_us={editor_p95} timeline_items=10000 frame=120x40 profile=debug"
    );

    assert!(render_p95 < 50_000, "render p95 was {render_p95} µs");
    assert!(editor_p95 < 4_000, "editor p95 was {editor_p95} µs");
}

#[test]
fn bounded_syntax_highlighting_stays_interactive() {
    let source = (0..64)
        .map(|index| {
            format!(
                "```rust\n// request {index}\nfn render_{index}(value: &str) -> usize {{\n    value.chars().count() + 42\n}}\n```"
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let _ = view::markdown_preview_at_width(&source, Theme::Dark, 100);
    let mut samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let lines = view::markdown_preview_at_width(&source, Theme::Dark, 100);
        assert_eq!(lines.len(), 384);
        samples.push(started.elapsed().as_micros());
    }
    samples.sort_unstable();
    let render_p95 = samples[28];
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    eprintln!(
        "TUI_SYNTAX_BASELINE render_p95_us={render_p95} blocks=64 lines=384 width=100 profile={profile}"
    );
    assert!(
        render_p95 < 150_000,
        "syntax render p95 was {render_p95} µs"
    );
}
