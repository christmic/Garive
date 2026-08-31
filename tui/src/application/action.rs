use super::{AppEffectResult, FocusTarget, Overlay, PendingMutationDraft, TerminalSize};

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
    CreateSessionRequested(PendingMutationDraft),
    StartTurnRequested(PendingMutationDraft),
    CancelTurnRequested(PendingMutationDraft),
    ContinueTurnRequested {
        draft: PendingMutationDraft,
        schema_digest: String,
    },
    #[allow(dead_code)]
    EffectFinished(AppEffectResult),
}
