use garive_host_client::HostClientErrorCode;

use crate::{
    application::{AppAction, BootState, ConnectionState, ExecutionState, Overlay},
    persistence::{DiagnosticEvent, PendingKind},
};

use super::{
    super::{
        controller::replay_pending,
        host::{self, HostMessage, HostOperation},
    },
    projection::{apply_event, apply_live_output, install_timeline},
    RuntimeState,
};

mod correlation;

use correlation::{
    contains_pending, matches_session_created, matches_turn_accepted, unique_pending,
};

pub(super) fn handle_host(message: HostMessage, state: &mut RuntimeState) {
    match message {
        HostMessage::SnapshotLoaded {
            request_id,
            session_id,
            view,
            items,
            follow_position,
        } if state.model.selected_session.as_deref() == Some(&session_id)
            && request_id == state.snapshot_request =>
        {
            install_timeline(&mut state.model, items);
            state.model.reconcile_inspector_surface();
            state.model.observed_position = follow_position;
            state.model.connection = ConnectionState::Online;
            state.model.session_count = state.model.sessions.len();
            if let Some(summary) = state
                .model
                .sessions
                .iter_mut()
                .find(|item| item.session_id == session_id)
            {
                *summary = view.session;
            }
            state.follow = Some(host::follow(
                state.client.clone(),
                session_id.clone(),
                follow_position,
                state.sender.clone(),
            ));
            state.live_follow = Some(host::follow_live(
                state.client.clone(),
                session_id.clone(),
                state.sender.clone(),
            ));
            replay_queued_for_session(state, &session_id);
        }
        HostMessage::SnapshotLoaded { .. } => {}
        HostMessage::SessionCreated {
            command_id,
            response,
        } => {
            if !matches_session_created(&state.pending, &command_id) {
                refresh_after_unmatched_mutation(state);
                return;
            }
            state.finish_pending(&command_id, "");
            if contains_pending(&state.pending, &command_id) {
                return;
            }
            let session_id = response.session_id;
            state.load(session_id.clone());
            if let Some(text) = state.queued_prompt.take() {
                let command_id = state.command_id("turn");
                state.request_start_turn(command_id, session_id, text);
            } else {
                state.model.composer.clear();
                state.model.prompt_history_browser.reset();
            }
            state.refresh_session_catalog();
        }
        HostMessage::TurnAccepted {
            command_id,
            session_id,
            submitted_text,
            response,
        } => {
            let Some(kind) = matches_turn_accepted(
                &state.pending,
                &command_id,
                &session_id,
                &submitted_text,
                &response.session_id,
                &response.turn_id,
            ) else {
                refresh_after_unmatched_mutation(state);
                return;
            };
            state.finish_pending(&command_id, &submitted_text);
            if contains_pending(&state.pending, &command_id) {
                return;
            }
            if kind == PendingKind::StartTurn && !submitted_text.is_empty() {
                state.model.composer.clear();
                state.model.prompt_history_browser.reset();
            }
            state.model.suspension_response = None;
            state.model.live_answer.clear_for_session_change();
            state.model.selected_turn = Some(response.turn_id);
            state.model.active_execution_id = Some(response.execution_id);
            state.model.execution = ExecutionState::Following;
            state.load(session_id);
        }
        HostMessage::Event(event) => apply_event(event, state),
        HostMessage::LiveOutput(event) => apply_live_output(event, state),
        HostMessage::LiveFollowEnded { session_id, code }
            if state.model.selected_session.as_deref() == Some(&session_id) =>
        {
            state.live_follow = None;
            let detached = !state.model.viewport.follow_latest;
            let effect = state.model.live_answer.preview_unavailable(detached);
            if effect.unseen_increment {
                state.model.viewport.newer_updates =
                    state.model.viewport.newer_updates.saturating_add(1);
            }
            let fatal = matches!(
                code,
                HostClientErrorCode::InvalidConfiguration
                    | HostClientErrorCode::InvalidCommand
                    | HostClientErrorCode::InvalidEvent
            );
            if !fatal
                && state.model.execution == ExecutionState::Following
                && state.live_reconnect_attempt < 5
            {
                state.live_reconnect_attempt += 1;
                state.live_reconnect = Some(host::schedule_live_reconnect(
                    session_id,
                    state.live_reconnect_attempt,
                    state.sender.clone(),
                ));
            }
        }
        HostMessage::LiveFollowEnded { .. } => {}
        HostMessage::LiveReconnectDue {
            session_id,
            attempt,
        } if state.model.selected_session.as_deref() == Some(&session_id)
            && state.live_reconnect_attempt == attempt
            && state.model.execution == ExecutionState::Following =>
        {
            state.live_reconnect = None;
            state.live_follow = Some(host::follow_live(
                state.client.clone(),
                session_id,
                state.sender.clone(),
            ));
        }
        HostMessage::LiveReconnectDue { .. } => {}
        HostMessage::FollowEnded { session_id, code }
            if state.model.selected_session.as_deref() == Some(&session_id) =>
        {
            state.follow = None;
            if matches!(
                code,
                HostClientErrorCode::InvalidEvent
                    | HostClientErrorCode::EventOrderViolation
                    | HostClientErrorCode::EventLimitExceeded
            ) {
                state.model.connection = ConnectionState::Unavailable {
                    safe_code: code.wire_name(),
                };
            } else if matches!(
                state.model.execution,
                ExecutionState::Following | ExecutionState::Suspended
            ) && state.reconnect_attempt < 5
            {
                state.reconnect_attempt += 1;
                state.model.connection = ConnectionState::Disconnected {
                    attempt: state.reconnect_attempt,
                };
                state.reconnect = Some(host::schedule_reconnect(
                    session_id,
                    state.reconnect_attempt,
                    state.sender.clone(),
                ));
            } else {
                state.model.connection = ConnectionState::Disconnected {
                    attempt: state.reconnect_attempt,
                };
            }
        }
        HostMessage::FollowEnded { session_id, code } => {
            let Some(background) = state.background_follows.get_mut(&session_id) else {
                return;
            };
            background.follow = None;
            if matches!(
                code,
                HostClientErrorCode::InvalidEvent
                    | HostClientErrorCode::EventOrderViolation
                    | HostClientErrorCode::EventLimitExceeded
            ) {
                state.background_follows.remove(&session_id);
                state.model.notice = Some(format!(
                    "Background Session follow stopped: {}.",
                    code.wire_name()
                ));
            } else if background.attempt < 5 {
                background.attempt += 1;
                background.reconnect = Some(host::schedule_reconnect(
                    session_id,
                    background.attempt,
                    state.sender.clone(),
                ));
            }
        }
        HostMessage::ReconnectDue {
            session_id,
            attempt,
        } if state.model.selected_session.as_deref() == Some(&session_id)
            && state.reconnect_attempt == attempt =>
        {
            state.reconnect = None;
            state.model.connection = ConnectionState::Reconnecting { attempt };
            state.follow = Some(host::follow(
                state.client.clone(),
                session_id,
                state.model.observed_position,
                state.sender.clone(),
            ));
        }
        HostMessage::ReconnectDue {
            session_id,
            attempt,
        } => {
            let Some(background) = state.background_follows.get_mut(&session_id) else {
                return;
            };
            if background.attempt != attempt {
                return;
            }
            background.reconnect = None;
            background.follow = Some(host::follow(
                state.client.clone(),
                session_id,
                background.observed_position,
                state.sender.clone(),
            ));
        }
        HostMessage::Failed { operation, error } => {
            if let HostOperation::Mutation { command_id } = &operation {
                if unique_pending(&state.pending, command_id).is_none() {
                    refresh_after_unmatched_mutation(state);
                    return;
                }
            }
            let refresh_failed = matches!(
                &operation,
                HostOperation::Snapshot { request_id } if *request_id == state.snapshot_request
            );
            if refresh_failed && state.cancel_exact_retry_refresh() {
                state.model.notice =
                    Some("Fresh Host truth could not be loaded; exact retry was not sent.".into());
            }
            let code = error.code;
            let _ = state.store.record_diagnostic(DiagnosticEvent::HostFailure {
                safe_code: code.wire_name(),
            });
            match operation {
                HostOperation::Snapshot { request_id } if request_id != state.snapshot_request => {}
                HostOperation::Snapshot { .. }
                    if matches!(
                        code,
                        HostClientErrorCode::InvalidConfiguration
                            | HostClientErrorCode::InvalidEvent
                            | HostClientErrorCode::EventOrderViolation
                            | HostClientErrorCode::EventLimitExceeded
                    ) =>
                {
                    state.dispatch(AppAction::HostUnavailable {
                        safe_code: code.wire_name(),
                    });
                    state.model.execution = ExecutionState::Failed;
                }
                HostOperation::Snapshot { .. } if error.status.is_some() => {
                    state.model.connection = ConnectionState::Online;
                    state.model.notice =
                        Some(format!("Snapshot refresh deferred: {}.", code.wire_name()));
                }
                HostOperation::Snapshot { .. } => {
                    state.model.connection = ConnectionState::Disconnected {
                        attempt: state.reconnect_attempt,
                    };
                }
                HostOperation::Mutation { command_id } if error.status.is_some() => {
                    state.reject_pending(&command_id, code);
                    state.model.connection = ConnectionState::Online;
                    state.model.notice =
                        Some(format!("Host rejected the command: {}.", code.wire_name()));
                    if state.model.overlay != Some(Overlay::UnknownCommand) {
                        state.model.overlay = Some(Overlay::ErrorDetails);
                    }
                }
                HostOperation::Mutation { command_id } => {
                    state.reject_pending(&command_id, code);
                    state.model.connection = ConnectionState::Disconnected {
                        attempt: state.reconnect_attempt,
                    };
                }
            }
        }
    }
}

fn refresh_after_unmatched_mutation(state: &mut RuntimeState) {
    let _ = state.store.record_diagnostic(DiagnosticEvent::HostFailure {
        safe_code: HostClientErrorCode::InvalidEvent.wire_name(),
    });
    state.model.notice =
        Some("Ignored an unmatched Host response; refreshing durable truth.".into());
    if state.model.connection == ConnectionState::Connecting {
        return;
    }
    if let Some(session_id) = state.model.selected_session.clone() {
        state.load(session_id);
    } else {
        state.model.connection = ConnectionState::Connecting;
        state.refresh_session_catalog();
    }
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;

fn replay_queued_create(state: &mut RuntimeState) -> bool {
    let Some(pending) =
        state.claim_exact_retry_after_refresh(None, Some(PendingKind::CreateSession))
    else {
        return false;
    };
    replay_pending(state, pending);
    true
}

pub(super) fn apply_boot_completion(state: &mut RuntimeState) {
    if !matches!(
        state.model.boot,
        BootState::Ready | BootState::NotConfigured
    ) {
        return;
    }
    let selected = state
        .config
        .session
        .clone()
        .or_else(|| state.preferences.selected_session_id.clone())
        .or_else(|| {
            state
                .model
                .sessions
                .first()
                .map(|item| item.session_id.clone())
        });
    if let Some(id) = selected {
        state.load(id);
    }
    replay_queued_create(state);
}

pub(super) fn apply_catalog_refresh_completion(state: &mut RuntimeState) {
    if state.model.catalog_refresh_succeeded {
        replay_queued_create(state);
    } else if state.cancel_exact_retry_refresh() {
        state.model.notice =
            Some("Fresh Host truth could not be loaded; exact retry was not sent.".into());
    }
}

fn replay_queued_for_session(state: &mut RuntimeState, session_id: &str) -> bool {
    let Some(pending) = state.claim_exact_retry_after_refresh(Some(session_id), None) else {
        return false;
    };
    replay_pending(state, pending);
    true
}
