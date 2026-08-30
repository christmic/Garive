use super::{
    AppAction, AppEffect, AppModel, BootState, ConnectionState, EffectKind, EffectValue,
    FocusTarget, Overlay, PendingEffect,
};

pub(crate) fn reduce(model: &mut AppModel, action: AppAction) -> Vec<AppEffect> {
    model.dirty = true;
    match action {
        AppAction::Boot if model.boot == BootState::Cold => boot(model),
        AppAction::Boot => Vec::new(),
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
            vec![issue(model, EffectKind::Exit)]
        }
        AppAction::QuitConfirmed => Vec::new(),
        AppAction::EffectFinished(result) => finish(model, result),
    }
}

fn boot(model: &mut AppModel) -> Vec<AppEffect> {
    model.generation = model.generation.saturating_add(1);
    model.boot = BootState::Loading;
    model.connection = ConnectionState::Connecting;
    [
        EffectKind::LoadPreferences,
        EffectKind::LoadPendingCommand,
        EffectKind::LoadDefinitions,
        EffectKind::LoadSessions,
    ]
    .into_iter()
    .map(|kind| issue(model, kind))
    .collect()
}

fn issue(model: &mut AppModel, kind: EffectKind) -> AppEffect {
    model.next_effect_id = model.next_effect_id.saturating_add(1);
    let effect = AppEffect {
        id: super::EffectId(model.next_effect_id),
        issued_generation: model.generation,
        kind,
    };
    model.pending_effects.insert(
        effect.id,
        PendingEffect {
            generation: effect.issued_generation,
            kind,
        },
    );
    effect
}

fn finish(model: &mut AppModel, result: super::EffectResult) -> Vec<AppEffect> {
    let Some(pending) = model.pending_effects.get(&result.effect_id).copied() else {
        model.stale_result_count = model.stale_result_count.saturating_add(1);
        return Vec::new();
    };
    if pending.generation != result.issued_generation || !correlates(pending.kind, &result.value) {
        model.stale_result_count = model.stale_result_count.saturating_add(1);
        return Vec::new();
    }
    model.pending_effects.remove(&result.effect_id);
    match result.value {
        EffectValue::DefinitionsLoaded { count } => model.definition_count = count,
        EffectValue::SessionsLoaded { count } => model.session_count = count,
        EffectValue::Failed { safe_code } => {
            model.boot = BootState::Degraded;
            model.connection = ConnectionState::Unavailable { safe_code };
        }
        EffectValue::PreferencesLoaded | EffectValue::PendingCommandLoaded => {}
    }
    if model.boot == BootState::Loading && model.pending_effects.is_empty() {
        model.boot = if model.definition_count == 0 {
            BootState::NotConfigured
        } else {
            BootState::Ready
        };
        model.connection = ConnectionState::Online;
    }
    Vec::new()
}

fn correlates(kind: EffectKind, value: &EffectValue) -> bool {
    matches!(
        (kind, value),
        (EffectKind::LoadPreferences, EffectValue::PreferencesLoaded)
            | (
                EffectKind::LoadPendingCommand,
                EffectValue::PendingCommandLoaded
            )
            | (
                EffectKind::LoadDefinitions,
                EffectValue::DefinitionsLoaded { .. }
            )
            | (EffectKind::LoadSessions, EffectValue::SessionsLoaded { .. })
            | (_, EffectValue::Failed { .. })
    )
}
