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

use application::{AppModel, ExecutionState, FocusTarget, Overlay, TerminalSize};
use ratatui::{buffer::Buffer, layout::Rect};

fn hint(model: &AppModel) -> String {
    let area = Rect::new(0, 0, 100, 24);
    let mut buffer = Buffer::empty(area);
    let _ = view::render_cached(
        model,
        Theme::Mono,
        area,
        &mut buffer,
        &mut view::RenderCache::default(),
    );
    (0..area.width)
        .map(|x| buffer[(x, area.height - 1)].symbol())
        .collect::<String>()
        .trim()
        .to_owned()
}

#[test]
fn running_composer_keeps_recovery_selection_suggestion_limit_and_notice_priorities() {
    let mut model = AppModel {
        notice: Some("ordinary notice".into()),
        ..Default::default()
    };
    assert_eq!(hint(&model), "● ordinary notice");

    model.execution = ExecutionState::Following;
    model.composer.replace(&"x".repeat(3_585)).unwrap();
    assert!(hint(&model).contains("3585 of 4096 bytes"));

    model.composer.replace("/theme ").unwrap();
    model.terminal_size = TerminalSize {
        width: 100,
        height: 24,
    };
    assert!(hint(&model).contains("Tab complete command"));

    model.composer.move_document_start(false);
    model.composer.move_right(true);
    assert!(hint(&model).contains("Alt+C copy selection"));

    assert!(hint(&model).contains("Alt+C copy selection"));

    model.pending_recovery.current_session = true;
    assert!(hint(&model).contains("Ctrl+P open recovery actions"));

    model.overlay = Some(Overlay::Help);
    assert!(hint(&model).is_empty());
    model.overlay = Some(Overlay::UnknownCommand);
    assert!(hint(&model).is_empty());
    model.overlay = None;
    assert!(hint(&model).contains("Ctrl+P open recovery actions"));
}

#[test]
fn connection_recovery_outranks_editing_feedback_and_navigation_is_last() {
    let mut model = AppModel {
        connection: application::ConnectionState::Disconnected { attempt: 2 },
        execution: ExecutionState::Following,
        focus: FocusTarget::Conversation,
        ..Default::default()
    };
    assert!(hint(&model).contains("/reconnect resume events"));
    assert!(hint(&model).contains("Updates paused · attempt 2/5"));

    model.connection = application::ConnectionState::Online;
    model.execution = ExecutionState::Idle;
    assert!(hint(&model).contains("PgUp browse history"));
}

#[test]
fn running_submit_boundary_notice_is_visible_because_cancel_lives_in_the_composer_rail() {
    let model = AppModel {
        execution: ExecutionState::Following,
        notice: Some("Current Turn is running · draft retained".into()),
        ..Default::default()
    };
    assert_eq!(hint(&model), "● Current Turn is running · draft retained");
}
