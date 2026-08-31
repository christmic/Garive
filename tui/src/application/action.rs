use garive_host_client::SessionSummary;

use super::{
    AppEffectResult, FocusTarget, Overlay, PendingMutationDraft, SessionPageRequest, TerminalSize,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppAction {
    BootStarted,
    BootCompleted {
        definition_count: usize,
        session_count: usize,
    },
    SessionCatalogReplaced {
        sessions: Vec<SessionSummary>,
        next_before: Option<String>,
    },
    LoadSessionPageRequested(SessionPageRequest),
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
