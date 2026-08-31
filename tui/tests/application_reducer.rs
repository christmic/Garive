#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;

use application::{
    reduce, AppAction, AppModel, BootState, ConnectionState, ConversationLandmark, FocusTarget,
    Overlay, TerminalSize, TimelineItem, TimelineRole,
};

#[test]
fn boot_transitions_are_explicit_and_complete() {
    let mut model = AppModel::default();
    assert!(reduce(&mut model, AppAction::BootStarted).is_empty());
    assert_eq!(model.boot, BootState::Loading);
    reduce(
        &mut model,
        AppAction::BootCompleted {
            definition_count: 2,
            session_count: 3,
        },
    );
    assert_eq!(model.boot, BootState::Ready);
    assert_eq!(model.connection, ConnectionState::Online);
    assert_eq!((model.definition_count, model.session_count), (2, 3));
}

#[test]
fn unavailable_host_has_a_safe_public_code() {
    let mut model = AppModel::default();
    reduce(
        &mut model,
        AppAction::HostUnavailable {
            safe_code: "protocol_error",
        },
    );
    assert_eq!(model.boot, BootState::Degraded);
    assert_eq!(
        model.connection,
        ConnectionState::Unavailable {
            safe_code: "protocol_error"
        }
    );
}

#[test]
fn switching_sessions_restores_a_bounded_independent_viewport() {
    let mut model = AppModel {
        selected_session: Some("session-a".into()),
        ..Default::default()
    };
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some("a-anchor".into());
    model.switch_viewport("session-b");
    model.selected_session = Some("session-b".into());
    assert!(model.viewport.follow_latest);
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some("b-anchor".into());

    model.switch_viewport("session-a");
    assert_eq!(model.viewport.anchor_key.as_deref(), Some("a-anchor"));
    assert!(!model.viewport.follow_latest);

    let mut bounded = AppModel::default();
    for index in 0..80 {
        let session = format!("session-{index}");
        bounded.switch_viewport(&session);
        bounded.selected_session = Some(session);
    }
    assert!(bounded.session_viewports.len() <= 64);
    assert!(bounded.viewport_order.len() <= 64);
}

#[test]
fn blocking_overlays_own_focus_and_quit_requires_confirmation() {
    let mut model = AppModel::default();
    reduce(
        &mut model,
        AppAction::FocusChanged(FocusTarget::Conversation),
    );
    reduce(&mut model, AppAction::OverlayOpened(Overlay::Suspension));
    reduce(&mut model, AppAction::OverlayClosed);
    reduce(&mut model, AppAction::QuitRequested);
    assert_eq!(model.overlay, Some(Overlay::Suspension));
    assert!(reduce(&mut model, AppAction::QuitConfirmed).is_empty());

    model.overlay = None;
    reduce(&mut model, AppAction::QuitRequested);
    let exit = reduce(&mut model, AppAction::QuitConfirmed);
    assert!(model.quit_requested);
    assert_eq!(exit.len(), 1);
}

#[test]
fn every_terminal_size_is_representable_without_underflow() {
    let mut model = AppModel::default();
    for size in [
        TerminalSize {
            width: 0,
            height: 0,
        },
        TerminalSize {
            width: 19,
            height: 7,
        },
        TerminalSize {
            width: 20,
            height: 8,
        },
        TerminalSize {
            width: u16::MAX,
            height: u16::MAX,
        },
    ] {
        reduce(&mut model, AppAction::TerminalResized(size));
        assert_eq!(
            model.terminal_size.is_supported(),
            size.width >= 20 && size.height >= 8
        );
    }
}

#[test]
fn turn_navigation_uses_public_positions_and_escape_or_focus_loss_is_non_mutating() {
    let mut model = AppModel {
        focus: FocusTarget::Conversation,
        prior_focus: FocusTarget::Conversation,
        overlay: Some(Overlay::TurnNavigator),
        turn_filter: "beta".into(),
        conversation_landmarks: vec![landmark(1, 10, "alpha"), landmark(2, 20, "beta")],
        ..Default::default()
    };
    model.push_test_timeline_item(cell("alpha", 10));
    model.push_test_timeline_item(cell("beta", 20));
    model.viewport.follow_latest = false;
    model.viewport.anchor_key = Some("alpha".into());
    let original = model.viewport.clone();

    reduce(&mut model, AppAction::OverlayClosed);
    assert_eq!(model.viewport, original);
    assert!(model.turn_filter.is_empty());
    assert_eq!(model.overlay, None);

    assert!(model.jump_to_turn_position(20));
    assert!(model.viewport.follow_latest);
    model.overlay = Some(Overlay::TurnNavigator);
    model.turn_filter = "stale".into();
    reduce(&mut model, AppAction::TerminalFocusChanged(false));
    assert_eq!(model.overlay, None);
    assert!(model.turn_filter.is_empty());
}

fn landmark(ordinal: usize, started_position: u64, prompt: &str) -> ConversationLandmark {
    ConversationLandmark {
        ordinal,
        started_position,
        prompt_preview: prompt.into(),
    }
}

fn cell(key: &str, position: u64) -> TimelineItem {
    TimelineItem {
        stable_key: key.into(),
        position,
        role: TimelineRole::User,
        tone: Default::default(),
        text: key.into(),
    }
}
