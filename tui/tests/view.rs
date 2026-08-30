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

use application::{AppModel, BootState, FocusTarget, Overlay, TimelineItem, TimelineRole};
use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};

fn frame(model: &AppModel, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    (0..height)
        .map(|y| {
            let line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn minimum_and_compact_frames_are_truthful() {
    let model = AppModel::default();
    assert!(frame(&model, 19, 7).contains("20×8"));
    let compact = frame(&model, 60, 12);
    assert!(compact.contains("Connecting to your durable workspace"));
    assert!(compact.contains("Enter send"));
    assert!(!compact.contains("Sessions ("));
}

#[test]
fn standard_frame_has_navigation_timeline_and_safe_text() {
    let mut model = AppModel {
        boot: BootState::Ready,
        session_count: 3,
        selected_session: Some("session-1234567890".into()),
        ..Default::default()
    };
    model.timeline.push(TimelineItem {
        stable_key: "answer".into(),
        position: 7,
        role: TimelineRole::Agent,
        text: "answer\u{1b}[31m\u{2066}x\u{2069}".into(),
    });
    let standard = frame(&model, 120, 18);
    assert!(standard.contains("Sessions 3"));
    assert!(standard.contains("session-1234"));
    assert!(standard.contains("answer�[31m⟦LRI⟧x⟦PDI⟧"));
    assert!(!standard.contains('\u{1b}'));
}

#[test]
fn overlay_is_rendered_above_without_mutating_model() {
    let model = AppModel {
        overlay: Some(Overlay::QuitConfirmation),
        ..Default::default()
    };
    let before = format!("{model:?}");
    let rendered = frame(&model, 80, 16);
    assert!(rendered.contains("Quit Garive?"));
    assert_eq!(format!("{model:?}"), before);
}

#[test]
fn modal_hierarchy_dims_the_workspace_and_highlights_selection() {
    let model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        ..Default::default()
    };
    let area = Rect::new(0, 0, 100, 24);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        &model,
        Theme::Dark,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
    let selected = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .find(|&(x, y)| buffer[(x, y)].symbol() == "›")
        .expect("selected command marker");
    assert_ne!(buffer[selected].bg, buffer[(selected.0, selected.1 + 1)].bg);
}

#[test]
fn only_the_composer_owns_the_terminal_cursor() {
    let mut model = AppModel::default();
    let area = Rect::new(0, 0, 100, 24);
    let mut buffer = Buffer::empty(area);
    assert!(view::render_cached(
        &model,
        Theme::Dark,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .is_some());

    model.focus = FocusTarget::Conversation;
    let mut buffer = Buffer::empty(area);
    assert!(view::render_cached(
        &model,
        Theme::Dark,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .is_none());
}

#[test]
fn searchable_overlays_show_only_matching_rows() {
    let mut model = AppModel {
        overlay: Some(Overlay::CommandPalette),
        command_filter: "copy completion".into(),
        ..Default::default()
    };
    let palette = frame(&model, 100, 24);
    assert!(palette.contains("/copy last"));
    assert!(!palette.contains("/status"));

    model.overlay = Some(Overlay::PromptHistory);
    model.history_filter = "deploy".into();
    model.prompt_history = vec!["deploy release".into(), "write tests".into()];
    let history = frame(&model, 100, 24);
    assert!(history.contains("deploy release"));
    assert!(!history.contains("write tests"));
}

#[test]
fn agent_markdown_is_structured_and_terminal_safe() {
    let mut model = AppModel {
        boot: BootState::Ready,
        ..Default::default()
    };
    model.timeline.push(TimelineItem {
        stable_key: "markdown".into(),
        position: 1,
        role: TimelineRole::Agent,
        text: "## Result\n\n- **done**\n- `cargo test`\n\n> <script>bad</script>\u{1b}[2J".into(),
    });

    let rendered = frame(&model, 100, 20);
    assert!(rendered.contains("## Result"));
    assert!(rendered.contains("• done"));
    assert!(rendered.contains("• cargo test"));
    assert!(rendered.contains("│ <script>bad</script>�[2J"));
    assert!(!rendered.contains('\u{1b}'));
}

#[test]
fn multiline_composer_keeps_the_cursor_inside_its_scrolled_viewport() {
    let mut model = AppModel::default();
    model
        .composer
        .replace("one\ntwo\nthree\nfour\nfive")
        .unwrap();
    let area = Rect::new(0, 0, 40, 12);
    let mut buffer = Buffer::empty(area);
    let cursor = view::render_cached(
        &model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    )
    .unwrap();
    assert!(cursor.1 < area.height - 1);
    assert!(frame(&model, 40, 12).contains("five"));
}
