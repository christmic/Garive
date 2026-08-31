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
        AppAction::CreateSessionRequested(draft)
            if draft.kind == PendingMutationKind::CreateSession
                && draft.session_id.is_none()
                && draft
                    .request_payload
                    .get("agent_definition_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|definition| !definition.is_empty())
                && !model
                    .effects
                    .pending
                    .values()
                    .any(|effect| effect.context.session_id.is_none()) =>
        {
            let effect = model.effects.issue(
                EffectKind::PersistPending {
                    draft: draft.clone(),
                },
                None,
                None,
            );
            if effect.is_some() {
                model.has_pending_command = true;
                if model.selected_session.is_none() {
                    model.composer_is_frozen = true;
                }
            }
            effect.into_iter().collect()
        }
        AppAction::CreateSessionRequested(_) => Vec::new(),
        AppAction::CancelTurnRequested(draft)
            if draft.kind == PendingMutationKind::CancelTurn
                && draft.session_id.as_deref() == model.selected_session.as_deref()
                && draft.turn_id.as_deref() == model.selected_turn.as_deref()
                && draft.suspension_id.is_none()
                && draft.expected_session_version.is_none()
                && draft
                    .requested_through_position
                    .is_some_and(|value| value > 0)
                && matches!(model.execution, ExecutionState::Following)
                && !has_pending_for_session(model, draft.session_id.as_deref())
                && !model.composer_is_frozen =>
        {
            issue_pending(model, draft, None)
        }
        AppAction::CancelTurnRequested(_) => Vec::new(),
        AppAction::ContinueTurnRequested {
            draft,
            schema_digest,
        } if draft.kind == PendingMutationKind::ContinueTurn
            && continuation_matches(model, &draft, &schema_digest)
            && draft.request_payload.get("input_json").is_some()
            && matches!(model.execution, ExecutionState::Suspended)
            && !has_pending_for_session(model, draft.session_id.as_deref())
            && !model.composer_is_frozen =>
        {
            let session_id = draft.session_id.clone();
            let effect = model.effects.issue(
                EffectKind::PersistContinuation {
                    draft,
                    schema_digest,
                },
                session_id,
                None,
            );
            if effect.is_some() {
                model.has_pending_command = true;
                model.composer_is_frozen = true;
            }
            effect.into_iter().collect()
        }
        AppAction::ContinueTurnRequested { .. } => Vec::new(),
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
                ) if draft.command_id == identity.command_id
                    && draft.kind == PendingMutationKind::StartTurn =>
                {
                    let mut context = effect.context;
                    context.request_digest = Some(identity.request_digest.clone());
                    vec![AppEffect {
                        context,
                        kind: EffectKind::StartTurn { draft, identity },
                    }]
                }
                (
                    EffectKind::PersistPending { draft },
                    AppEffectOutcome::PendingPersisted(Ok(identity)),
                ) if draft.command_id == identity.command_id
                    && draft.kind == PendingMutationKind::CreateSession =>
                {
                    let mut context = effect.context;
                    context.request_digest = Some(identity.request_digest.clone());
                    vec![AppEffect {
                        context,
                        kind: EffectKind::CreateSession { draft, identity },
                    }]
                }
                (
                    EffectKind::PersistPending { draft },
                    AppEffectOutcome::PendingPersisted(Ok(identity)),
                ) if draft.command_id == identity.command_id
                    && draft.kind == PendingMutationKind::CancelTurn =>
                {
                    let mut context = effect.context;
                    context.request_digest = Some(identity.request_digest.clone());
                    vec![AppEffect {
                        context,
                        kind: EffectKind::CancelTurn { draft, identity },
                    }]
                }
                (
                    EffectKind::PersistContinuation {
                        draft,
                        schema_digest,
                    },
                    AppEffectOutcome::PendingPersisted(Ok(identity)),
                ) if draft.command_id == identity.command_id => {
                    let host_allowed = continuation_matches(model, &draft, &schema_digest);
                    let mut context = effect.context;
                    context.request_digest = Some(identity.request_digest.clone());
                    vec![AppEffect {
                        context,
                        kind: EffectKind::ContinueTurn {
                            draft,
                            identity,
                            schema_digest,
                            host_allowed,
                        },
                    }]
                }
                (EffectKind::PersistPending { .. }, _) => {
                    model.has_pending_command = false;
                    model.composer_is_frozen = false;
                    model.notice = Some("The pending command could not be saved.".into());
                    model.overlay = Some(Overlay::ErrorDetails);
                    Vec::new()
                }
                (EffectKind::PersistContinuation { .. }, _) => {
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

fn issue_pending(
    model: &mut AppModel,
    draft: super::PendingMutationDraft,
    request_digest: Option<String>,
) -> Vec<AppEffect> {
    let session_id = draft.session_id.clone();
    let effect = model.effects.issue(
        EffectKind::PersistPending { draft },
        session_id,
        request_digest,
    );
    if effect.is_some() {
        model.has_pending_command = true;
        model.composer_is_frozen = true;
    }
    effect.into_iter().collect()
}

fn continuation_matches(
    model: &AppModel,
    draft: &super::PendingMutationDraft,
    digest: &str,
) -> bool {
    let Some(suspension) = model.suspension.as_ref() else {
        return false;
    };
    draft.session_id.as_deref() == model.selected_session.as_deref()
        && draft.turn_id.as_deref() == model.selected_turn.as_deref()
        && draft.suspension_id.as_deref() == Some(suspension.suspension_id.as_str())
        && draft.expected_session_version == Some(suspension.session_version)
        && suspension.response_schema_digest.as_deref() == Some(digest)
        && draft.requested_through_position.is_none()
}

fn has_pending_for_session(model: &AppModel, session_id: Option<&str>) -> bool {
    model.effects.pending.values().any(|effect| {
        effect.context.session_id.as_deref() == session_id
            && matches!(
                effect.kind,
                EffectKind::PersistPending { .. } | EffectKind::PersistContinuation { .. }
            )
    })
}
