use super::{
    AppAction, AppEffect, AppModel, BootState, ConnectionState, EffectKind, FocusTarget, Overlay,
};

pub(crate) fn reduce(model: &mut AppModel, action: AppAction) -> Vec<AppEffect> {
    match action {
        AppAction::BootStarted => {
            model.boot = BootState::Loading;
            model.connection = ConnectionState::Connecting;
            Vec::new()
        }
        AppAction::BootCompleted {
            definition_count,
            session_count,
        } => {
            model.definition_count = definition_count;
            model.session_count = session_count;
            model.boot = if definition_count == 0 {
                BootState::NotConfigured
            } else {
                BootState::Ready
            };
            model.connection = ConnectionState::Online;
            Vec::new()
        }
        AppAction::HostUnavailable { safe_code } => {
            model.boot = BootState::Degraded;
            model.connection = ConnectionState::Unavailable { safe_code };
            Vec::new()
        }
        AppAction::TerminalResized(size) => {
            model.terminal_size = size;
            Vec::new()
        }
        AppAction::TerminalFocusChanged(focused) => {
            model.terminal_focused = focused;
            Vec::new()
        }
        AppAction::FocusChanged(focus) if model.overlay.is_none() => {
            model.focus = focus;
            Vec::new()
        }
        AppAction::FocusChanged(_) => Vec::new(),
        AppAction::OverlayOpened(overlay) => {
            if model.overlay.is_none() {
                model.prior_focus = model.focus;
                model.focus = FocusTarget::Overlay;
                model.overlay = Some(overlay);
            }
            Vec::new()
        }
        AppAction::OverlayClosed => {
            if !model.overlay.is_some_and(Overlay::is_blocking) {
                model.overlay = None;
                model.focus = model.prior_focus;
            }
            Vec::new()
        }
        AppAction::QuitRequested => {
            if !model.overlay.is_some_and(Overlay::is_blocking) {
                model.prior_focus = model.focus;
                model.focus = FocusTarget::Overlay;
                model.overlay = Some(Overlay::QuitConfirmation);
            }
            Vec::new()
        }
        AppAction::QuitConfirmed if model.overlay == Some(Overlay::QuitConfirmation) => {
            model.quit_requested = true;
            vec![AppEffect {
                kind: EffectKind::Exit,
            }]
        }
        AppAction::QuitConfirmed => Vec::new(),
    }
}
