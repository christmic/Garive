#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;

use application::{
    reduce, AppAction, AppModel, BootState, ConnectionState, FocusTarget, Overlay, TerminalSize,
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
