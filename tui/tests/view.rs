#[path = "../src/args.rs"]
mod args;
pub use args::Theme;
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;
#[path = "../src/view/mod.rs"]
mod view;

use application::{AppModel, BootState, Overlay, TimelineItem, TimelineRole};
use ratatui::{buffer::Buffer, layout::Rect};

fn frame(model: &AppModel, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render(model, Theme::Mono, area, &mut buffer);
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
