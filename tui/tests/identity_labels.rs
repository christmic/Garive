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

use application::{AppModel, BootState, Overlay};
use garive_host_client::SessionSummary;
use ratatui::{buffer::Buffer, layout::Rect};

const PRIVATE_DEFINITION: &str = "internal-👨‍👩‍👧‍👦-e\u{301}-\u{1b}[31m-\u{2066}secret\u{2069}";
const PRIVATE_SESSION: &str = "session-🦀-\u{1b}[32m-private";

#[test]
fn opaque_identities_use_neutral_visual_and_linear_labels() {
    let mut model = AppModel {
        boot: BootState::Ready,
        selected_session: Some(PRIVATE_SESSION.into()),
        sessions: vec![session()],
        overlay: Some(Overlay::SessionPicker),
        session_filter: "secret".into(),
        ..Default::default()
    };

    let visual = frame(&model, 80, 16);
    let linear = view::linear_overlay(&model);
    for output in [&visual, &linear] {
        assert!(output.contains("Session 1"), "{output}");
        assert!(output.contains("Agent"), "{output}");
        assert!(!output.contains("internal"), "{output}");
        assert!(!output.contains("👨‍👩‍👧‍👦"), "{output}");
        assert!(!output.contains("e\u{301}"), "{output}");
        assert!(!output.contains("🦀"), "{output}");
        assert!(!output.contains('\u{1b}'), "{output}");
        assert!(!output.contains('\u{2066}'), "{output}");
        assert!(!output.contains('\u{2069}'), "{output}");
    }

    model.overlay = None;
    let context = frame(&model, 100, 16);
    assert!(context.contains("Session 1"), "{context}");
    assert!(!context.contains("Agent"), "{context}");
    assert!(!context.contains(PRIVATE_DEFINITION), "{context}");
    assert!(!context.contains(PRIVATE_SESSION), "{context}");
}

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
        .map(|row| {
            (0..width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn session() -> SessionSummary {
    SessionSummary {
        api_version: "v1".into(),
        session_id: PRIVATE_SESSION.into(),
        agent_instance_id: "private-agent-instance".into(),
        definition_id: PRIVATE_DEFINITION.into(),
        definition_revision: "private-revision".into(),
        opened_at: "2026-08-30T00:00:00Z".into(),
        latest_position: 1,
        latest_turn_id: Some("private-turn".into()),
        latest_turn_state: Some("running".into()),
        turn_count: 1,
    }
}
