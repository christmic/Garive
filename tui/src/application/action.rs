use super::{FocusTarget, Overlay, TerminalSize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppAction {
    BootStarted,
    BootCompleted {
        definition_count: usize,
        session_count: usize,
    },
    HostUnavailable {
        safe_code: &'static str,
    },
    TerminalResized(TerminalSize),
    TerminalFocusChanged(bool),
    FocusChanged(FocusTarget),
    OverlayOpened(Overlay),
    OverlayClosed,
    QuitRequested,
    QuitConfirmed,
}
