use super::{EffectId, FocusTarget, Overlay, TerminalSize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppAction {
    Boot,
    TerminalResized(TerminalSize),
    TerminalFocusChanged(bool),
    FocusChanged(FocusTarget),
    OverlayOpened(Overlay),
    OverlayClosed,
    QuitRequested,
    QuitConfirmed,
    EffectFinished(EffectResult),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectResult {
    pub(crate) effect_id: EffectId,
    pub(crate) issued_generation: u64,
    pub(crate) value: EffectValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EffectValue {
    PreferencesLoaded,
    PendingCommandLoaded,
    DefinitionsLoaded { count: usize },
    SessionsLoaded { count: usize },
    Failed { safe_code: &'static str },
}
