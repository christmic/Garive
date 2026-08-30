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

use application::{
    AppModel, BootState, ConnectionState, ExecutionState, Overlay, TimelineItem, TimelineRole,
};
use garive_host_client::{AgentDefinitionSummary, SessionSummary};
use ratatui::{buffer::Buffer, layout::Rect};

#[test]
fn responsive_product_frames_match_reviewed_snapshots() {
    let model = product_model();
    insta::assert_snapshot!("compact_40x12", frame(&model, Theme::Mono, 40, 12));
    insta::assert_snapshot!("standard_100x24", frame(&model, Theme::Dark, 100, 24));

    let mut wide = model;
    wide.overlay = Some(Overlay::CommandPalette);
    insta::assert_snapshot!("wide_palette_160x28", frame(&wide, Theme::Light, 160, 28));
}

#[test]
fn boundary_size_and_theme_matrix_never_panics() {
    let model = product_model();
    for theme in [Theme::System, Theme::Dark, Theme::Light, Theme::Mono] {
        for (width, height) in [
            (0, 0),
            (1, 1),
            (19, 7),
            (20, 8),
            (40, 8),
            (40, 12),
            (79, 23),
            (80, 24),
            (99, 24),
            (100, 24),
            (159, 28),
            (160, 28),
            (200, 40),
        ] {
            let rendered = frame(&model, theme, width, height);
            assert!(!rendered.contains('\u{1b}'));
        }
    }
}

fn product_model() -> AppModel {
    let mut model = AppModel {
        boot: BootState::Ready,
        definition_count: 1,
        definitions: vec![AgentDefinitionSummary {
            api_version: "v1".into(),
            definition_id: "research-agent".into(),
            definition_revision: "revision-1".into(),
            capabilities: vec!["web".into()],
        }],
        session_count: 2,
        sessions: vec![
            session("session-alpha-123456", "completed", 3),
            session("session-beta-987654", "running", 1),
        ],
        selected_session: Some("session-alpha-123456".into()),
        connection: ConnectionState::Online,
        execution: ExecutionState::Following,
        observed_position: 42,
        ..Default::default()
    };
    model.timeline = vec![
        item("user", 2, TimelineRole::User, "Summarize the release plan."),
        item(
            "activity",
            4,
            TimelineRole::Status,
            "activity.research · completed",
        ),
        item(
            "agent",
            6,
            TimelineRole::Agent,
            "## Release plan\n\n- Verify the Runtime\n- Ship with **evidence**\n\n`cargo test` passes.",
        ),
    ];
    model.composer.replace("Ask a follow-up…").unwrap();
    model
}

fn session(id: &str, state: &str, turns: u64) -> SessionSummary {
    SessionSummary {
        api_version: "v1".into(),
        session_id: id.into(),
        agent_instance_id: "agent-instance".into(),
        definition_id: "research-agent".into(),
        definition_revision: "revision-1".into(),
        opened_at: "2026-08-30T00:00:00Z".into(),
        latest_position: 42,
        latest_turn_id: Some("turn-1".into()),
        latest_turn_state: Some(state.into()),
        turn_count: turns,
    }
}

fn item(key: &str, position: u64, role: TimelineRole, text: &str) -> TimelineItem {
    TimelineItem {
        stable_key: key.into(),
        position,
        role,
        text: text.into(),
    }
}

fn frame(model: &AppModel, theme: Theme, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let _ = view::render(model, theme, area, &mut buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
