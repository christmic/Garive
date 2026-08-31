use super::{
    AppEffectResult, FocusTarget, Overlay, PendingMutationDraft, SessionPageRequest,
    SnapshotRequest, TerminalSize,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppAction {
    BootStarted,
    LoadSessionPageRequested(SessionPageRequest),
    LoadSnapshotRequested(SnapshotRequest),
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
