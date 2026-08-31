use super::{
    AppAction, AppEffect, AppEffectOutcome, AppModel, BootState, ConnectionState, EffectKind,
    ExecutionState, FocusTarget, Overlay, PendingMutationKind,
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
            model.reconcile_inspector_surface();
            Vec::new()
        }
        AppAction::TerminalFocusChanged(focused) => {
            if !focused {
                model.close_turn_navigator();
            }
            model.terminal_focused = focused;
            Vec::new()
        }
        AppAction::FocusChanged(focus) if model.overlay.is_none() => {
            model.focus = focus;
            if model.inspector.open {
                model.inspector.focus_owned = focus == FocusTarget::Inspector;
            }
            Vec::new()
        }
        AppAction::FocusChanged(_) => Vec::new(),
        AppAction::OverlayOpened(overlay) => {
            if model.overlay.is_none() || model.overlay == Some(Overlay::Inspector) {
                model.prior_focus = model.focus;
                model.focus = FocusTarget::Overlay;
                model.overlay = Some(overlay);
            }
            Vec::new()
        }
        AppAction::OverlayClosed => {
            if !model.overlay.is_some_and(Overlay::is_blocking) {
                if let Some(return_overlay) = model.return_overlay.take() {
                    model.overlay = Some(return_overlay);
                    model.focus = FocusTarget::Overlay;
                } else if model.overlay == Some(Overlay::Inspector) {
                    model.close_inspector();
                } else if model.overlay == Some(Overlay::TurnNavigator) {
                    model.close_turn_navigator();
                } else {
                    model.overlay = None;
                    model.focus = model.prior_focus;
                    model.reconcile_inspector_surface();
                }
            }
            Vec::new()
        }
        AppAction::QuitRequested => {
            model.close_turn_navigator();
            if !model.overlay.is_some_and(Overlay::is_blocking) {
                model.prior_focus = model.focus;
                model.focus = FocusTarget::Overlay;
                model.overlay = Some(Overlay::QuitConfirmation);
            }
            Vec::new()
        }
        AppAction::QuitConfirmed if model.overlay == Some(Overlay::QuitConfirmation) => {
            model.quit_requested = true;
            model
                .effects
                .issue(EffectKind::Exit, None, None)
                .into_iter()
                .collect()
        }
        AppAction::QuitConfirmed => Vec::new(),
        AppAction::StartTurnRequested(draft)
            if draft.kind == PendingMutationKind::StartTurn
                && draft.session_id.is_some()
                && draft.session_id.as_deref() == model.selected_session.as_deref()
                && draft
                    .request_payload
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty())
                && matches!(
                    model.execution,
                    ExecutionState::Idle | ExecutionState::Failed
                )
                && !model.composer_is_frozen =>
        {
            let effect = model.effects.issue(
                EffectKind::PersistPending {
                    draft: draft.clone(),
                },
                draft.session_id.clone(),
                None,
            );
            if effect.is_some() {
                model.has_pending_command = true;
                model.composer_is_frozen = true;
            }
            effect.into_iter().collect()
        }
        AppAction::StartTurnRequested(_) => Vec::new(),
        AppAction::EffectFinished(result) => {
            let Some(effect) = model.effects.take_finished(&result) else {
                return Vec::new();
            };
            match (effect.kind, result.outcome) {
                (
                    EffectKind::PersistPending { draft },
                    AppEffectOutcome::PendingPersisted(Ok(identity)),
                ) if draft.command_id == identity.command_id => {
                    let mut context = effect.context;
                    context.request_digest = Some(identity.request_digest.clone());
                    vec![AppEffect {
                        context,
                        kind: EffectKind::StartTurn { draft, identity },
                    }]
                }
                (EffectKind::PersistPending { .. }, _) => {
                    model.has_pending_command = false;
                    model.composer_is_frozen = false;
                    model.notice = Some("The pending command could not be saved.".into());
                    model.overlay = Some(Overlay::ErrorDetails);
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
    }
}
