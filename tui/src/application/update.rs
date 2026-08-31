use super::{
    bootstrap, snapshot, AppAction, AppEffect, AppEffectOutcome, AppModel, EffectKind,
    ExecutionState, FocusTarget, HostReadResponse, Overlay, PendingMutationKind, SessionPageOwner,
    SessionPagePurpose, SessionPageRequest, SnapshotOwner,
};

pub(crate) fn reduce(model: &mut AppModel, action: AppAction) -> Vec<AppEffect> {
    match action {
        AppAction::BootStarted => bootstrap::begin(model),
        AppAction::LoadSessionPageRequested(request)
            if session_page_request_is_admitted(model, &request)
                && model
                    .session_page_owner
                    .as_ref()
                    .is_none_or(|owner| owner.request != request) =>
        {
            let effect = model.effects.issue(
                EffectKind::LoadSessionPage {
                    request: request.clone(),
                },
                None,
                Some(request.identity_digest()),
            );
            if let Some(effect) = &effect {
                model.session_page_owner = Some(SessionPageOwner {
                    context: effect.context.clone(),
                    request,
                });
                model.sessions_loading = true;
            }
            effect.into_iter().collect()
        }
        AppAction::LoadSessionPageRequested(_) => Vec::new(),
        AppAction::LoadSnapshotRequested(request)
            if model.selected_session.as_deref() == Some(request.session_id.as_str()) =>
        {
            let effect = model.effects.issue(
                EffectKind::LoadSnapshot {
                    request: request.clone(),
                },
                Some(request.session_id.clone()),
                Some(request.identity_digest()),
            );
            if let Some(effect) = &effect {
                model.snapshot_owner = Some(SnapshotOwner {
                    context: effect.context.clone(),
                    request,
                });
            }
            effect.into_iter().collect()
        }
        AppAction::LoadSnapshotRequested(_) => Vec::new(),
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
                && !model.effects.has_pending_mutation_for_context(None) =>
        {
            let effect = model.effects.issue(
                EffectKind::PersistPending {
                    draft: draft.clone(),
                },
                None,
                None,
            );
            if effect.is_some() {
                sync_in_flight_pending_projection(model);
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
                sync_in_flight_pending_projection(model);
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
                sync_in_flight_pending_projection(model);
            }
            effect.into_iter().collect()
        }
        AppAction::StartTurnRequested(_) => Vec::new(),
        AppAction::EffectFinished(result) => {
            let Some(effect) = model.effects.take_finished(&result) else {
                return Vec::new();
            };
            sync_in_flight_pending_projection(model);
            if matches!(effect.kind, EffectKind::LoadDefinitions) {
                bootstrap::finish_definitions(model, effect.context, result.outcome);
                return Vec::new();
            }
            if let EffectKind::LoadSessionPage { request } = &effect.kind {
                return finish_session_page(model, effect.context, request.clone(), result.outcome);
            }
            if let EffectKind::LoadSnapshot { request } = &effect.kind {
                return snapshot::finish(model, effect.context, request.clone(), result.outcome);
            }
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
                    model.notice = Some("The pending command could not be saved.".into());
                    model.overlay = Some(Overlay::ErrorDetails);
                    Vec::new()
                }
                (EffectKind::PersistContinuation { .. }, _) => {
                    model.notice = Some("The pending command could not be saved.".into());
                    model.overlay = Some(Overlay::ErrorDetails);
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
    }
}

fn session_page_request_is_admitted(model: &AppModel, request: &SessionPageRequest) -> bool {
    match request.purpose {
        SessionPagePurpose::Replace => false,
        SessionPagePurpose::CatalogRefresh => request.cursor.is_none(),
        SessionPagePurpose::Append => {
            request.cursor.is_some() && request.cursor == model.sessions_next_before
        }
    }
}

fn finish_session_page(
    model: &mut AppModel,
    context: super::EffectContext,
    request: SessionPageRequest,
    outcome: AppEffectOutcome,
) -> Vec<AppEffect> {
    let is_active = model
        .session_page_owner
        .as_ref()
        .is_some_and(|owner| owner.context == context && owner.request == request);
    if !is_active {
        return Vec::new();
    }
    model.session_page_owner = None;
    model.sessions_loading = false;
    let succeeded = match outcome {
        AppEffectOutcome::HostRead(Ok(HostReadResponse::SessionPage {
            request: response_request,
            sessions,
            next_before,
        })) if response_request == request => {
            match request.purpose {
                SessionPagePurpose::Replace | SessionPagePurpose::CatalogRefresh => {
                    replace_session_catalog(model, sessions, next_before)
                }
                SessionPagePurpose::Append => append_session_catalog(model, sessions, next_before),
            }
            true
        }
        AppEffectOutcome::HostRead(Err(failure)) => {
            model.notice = Some(format!(
                "Session page unavailable: {}.",
                failure.code.wire_name()
            ));
            if request.purpose == SessionPagePurpose::Replace {
                bootstrap::finish_sessions(model, Err(failure.code.wire_name()));
            }
            false
        }
        _ => {
            model.notice = Some("Ignored an invalid Session page response.".into());
            if request.purpose == SessionPagePurpose::Replace {
                bootstrap::finish_sessions(model, Err("internal_failure"));
            }
            false
        }
    };
    match request.purpose {
        SessionPagePurpose::Replace => {
            if succeeded {
                bootstrap::finish_sessions(model, Ok(()));
            }
        }
        SessionPagePurpose::CatalogRefresh => {
            model.catalog_refresh_succeeded = succeeded;
            model.catalog_refresh_revision = model.catalog_refresh_revision.saturating_add(1);
        }
        SessionPagePurpose::Append => {}
    }
    Vec::new()
}

fn replace_session_catalog(
    model: &mut AppModel,
    sessions: Vec<garive_host_client::SessionSummary>,
    next_before: Option<String>,
) {
    let mut unique = Vec::with_capacity(sessions.len());
    for session in sessions {
        if !unique
            .iter()
            .any(|existing: &garive_host_client::SessionSummary| {
                existing.session_id == session.session_id
            })
        {
            unique.push(session);
        }
    }
    model.sessions = unique;
    model.sessions_next_before = next_before;
    model.sessions_loading = false;
    model.session_page_owner = None;
    model.session_count = model.sessions.len();
}

fn append_session_catalog(
    model: &mut AppModel,
    sessions: Vec<garive_host_client::SessionSummary>,
    next_before: Option<String>,
) {
    for session in sessions {
        if !model
            .sessions
            .iter()
            .any(|existing| existing.session_id == session.session_id)
        {
            model.sessions.push(session);
        }
    }
    model.sessions_next_before = next_before;
    model.session_count = model.sessions.len();
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
        sync_in_flight_pending_projection(model);
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
    model.effects.has_pending_mutation_for_context(session_id)
}

fn sync_in_flight_pending_projection(model: &mut AppModel) {
    model.has_pending_command = model.effects.has_pending_mutation();
    model.composer_is_frozen = model
        .effects
        .has_pending_mutation_for_context(model.selected_session.as_deref());
}
