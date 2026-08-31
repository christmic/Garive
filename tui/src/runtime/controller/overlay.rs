use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::application::{ActionOverlayIntent, ActionOverlayKey, AppAction, Overlay};
use crate::input::{response_schema_control, SchemaControl};

use super::{
    actions::{retry_pending, submit_suspension_response},
    navigation::{
        is_safe_query_character, matching_commands, matching_history, matching_landmarks,
        matching_sessions, select_command, select_history, select_landmark, select_session,
    },
    RuntimeState,
};

pub(super) fn handle(key: KeyEvent, state: &mut RuntimeState) -> bool {
    let Some(overlay) = state.model.overlay else {
        return false;
    };
    if let Some(intent) = action_overlay_key(key)
        .and_then(|normalized| {
            state
                .model
                .decision_bindings(overlay)?
                .iter()
                .find(|binding| binding.key == normalized)
        })
        .map(|binding| binding.intent)
    {
        activate_intent(intent, overlay, state);
        return true;
    }
    match key.code {
        KeyCode::Esc if overlay != Overlay::UnknownCommand => {
            state.dispatch(AppAction::OverlayClosed)
        }
        KeyCode::Up if overlay == Overlay::Inspector => {
            super::inspector::move_selection(state, true)
        }
        KeyCode::Down if overlay == Overlay::Inspector => {
            super::inspector::move_selection(state, false)
        }
        KeyCode::Home if overlay == Overlay::Inspector => super::inspector::select_index(state, 0),
        KeyCode::End if overlay == Overlay::Inspector => {
            let last = state
                .model
                .inspector_projection()
                .entries
                .len()
                .saturating_sub(1);
            super::inspector::select_index(state, last);
        }
        KeyCode::Enter if overlay == Overlay::Inspector => super::inspector::activate(state),
        KeyCode::Up if overlay == Overlay::SessionPicker => {
            state.model.session_selection = state.model.session_selection.saturating_sub(1)
        }
        KeyCode::Down if overlay == Overlay::SessionPicker => {
            let last = matching_sessions(state).len().saturating_sub(1);
            if state.model.session_selection >= last {
                state.load_more_sessions();
            } else {
                state.model.session_selection += 1;
            }
        }
        KeyCode::Tab if overlay == Overlay::SessionPicker => {
            super::navigation::cycle_session_selection(state, false)
        }
        KeyCode::BackTab if overlay == Overlay::SessionPicker => {
            super::navigation::cycle_session_selection(state, true)
        }
        KeyCode::Enter if overlay == Overlay::SessionPicker => select_session(state),
        KeyCode::Up if overlay == Overlay::TurnNavigator => {
            state.model.turn_selection = state.model.turn_selection.saturating_sub(1)
        }
        KeyCode::Down if overlay == Overlay::TurnNavigator => {
            state.model.turn_selection = (state.model.turn_selection + 1)
                .min(matching_landmarks(state).len().saturating_sub(1))
        }
        KeyCode::Home if overlay == Overlay::TurnNavigator => state.model.turn_selection = 0,
        KeyCode::End if overlay == Overlay::TurnNavigator => {
            state.model.turn_selection = matching_landmarks(state).len().saturating_sub(1)
        }
        KeyCode::Enter if overlay == Overlay::TurnNavigator => select_landmark(state),
        KeyCode::Up if overlay == Overlay::PromptHistory => {
            state.model.history_selection = state.model.history_selection.saturating_sub(1)
        }
        KeyCode::Down if overlay == Overlay::PromptHistory => {
            state.model.history_selection = (state.model.history_selection + 1)
                .min(matching_history(state).len().saturating_sub(1))
        }
        KeyCode::Enter if overlay == Overlay::PromptHistory => select_history(state),
        KeyCode::Up if overlay == Overlay::CommandPalette => {
            state.model.command_selection = state.model.command_selection.saturating_sub(1)
        }
        KeyCode::Down if overlay == Overlay::CommandPalette => {
            state.model.command_selection = (state.model.command_selection + 1)
                .min(matching_commands(state).len().saturating_sub(1))
        }
        KeyCode::Enter if overlay == Overlay::CommandPalette => select_command(state),
        KeyCode::Char(character)
            if overlay == Overlay::CommandPalette && is_safe_query_character(character) =>
        {
            state.model.command_filter.push(character);
            state.model.command_selection = 0;
        }
        KeyCode::Backspace if overlay == Overlay::CommandPalette => {
            state.model.command_filter.pop();
            state.model.command_selection = 0;
        }
        KeyCode::Char(character)
            if overlay == Overlay::SessionPicker && is_safe_query_character(character) =>
        {
            state.model.session_filter.push(character);
            state.model.session_selection = 0;
        }
        KeyCode::Backspace if overlay == Overlay::SessionPicker => {
            state.model.session_filter.pop();
            state.model.session_selection = 0;
        }
        KeyCode::Char(character)
            if overlay == Overlay::TurnNavigator && is_safe_query_character(character) =>
        {
            state.model.turn_filter.push(character);
            state.model.turn_selection = 0;
        }
        KeyCode::Backspace if overlay == Overlay::TurnNavigator => {
            state.model.turn_filter.pop();
            state.model.turn_selection = 0;
        }
        KeyCode::Char(character)
            if overlay == Overlay::PromptHistory && is_safe_query_character(character) =>
        {
            state.model.history_filter.push(character);
            state.model.history_selection = 0;
        }
        KeyCode::Backspace if overlay == Overlay::PromptHistory => {
            state.model.history_filter.pop();
            state.model.history_selection = 0;
        }
        KeyCode::Char(character)
            if overlay == Overlay::Suspension
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && suspension_uses_editor(state) =>
        {
            let response = state
                .model
                .suspension_response
                .as_mut()
                .expect("interactive suspension has response state");
            let _ = response.editor.insert(&character.to_string());
        }
        KeyCode::Up if overlay == Overlay::Suspension => move_suspension_choice(state, true),
        KeyCode::Down | KeyCode::Char(' ' | 'j') if overlay == Overlay::Suspension => {
            move_suspension_choice(state, false)
        }
        KeyCode::Backspace if overlay == Overlay::Suspension => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response.editor.backspace();
            }
        }
        KeyCode::Delete if overlay == Overlay::Suspension => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response.editor.delete();
            }
        }
        KeyCode::Left if overlay == Overlay::Suspension => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response
                    .editor
                    .move_left(key.modifiers.contains(KeyModifiers::SHIFT));
            }
        }
        KeyCode::Right if overlay == Overlay::Suspension => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response
                    .editor
                    .move_right(key.modifiers.contains(KeyModifiers::SHIFT));
            }
        }
        KeyCode::Home if overlay == Overlay::Suspension => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response.editor.move_document_start(false);
            }
        }
        KeyCode::End if overlay == Overlay::Suspension => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response.editor.move_document_end(false);
            }
        }
        _ => {}
    }
    true
}

pub(super) fn activate_intent(
    intent: ActionOverlayIntent,
    overlay: Overlay,
    state: &mut RuntimeState,
) {
    match intent {
        ActionOverlayIntent::Close => {
            if overlay == Overlay::EphemeralConfirmation {
                state.deferred_ephemeral = None;
                state.deferred_continuation_schema_digest = None;
            }
            state.dispatch(AppAction::OverlayClosed);
        }
        ActionOverlayIntent::ConfirmQuit => state.dispatch(AppAction::QuitConfirmed),
        ActionOverlayIntent::AcceptEphemeral => {
            state.ephemeral_confirmed = true;
            state.model.overlay = None;
            let deferred_schema_digest = state.deferred_continuation_schema_digest.take();
            if let Some(pending) = state.deferred_ephemeral.take() {
                if pending.kind == crate::persistence::PendingKind::StartTurn {
                    let text = pending
                        .request_payload
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    if let Some(session_id) = pending.session_id {
                        state.request_start_turn(pending.command_id, session_id, text);
                    }
                } else if pending.kind == crate::persistence::PendingKind::CreateSession {
                    let definition_id = pending
                        .request_payload
                        .get("agent_definition_id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    state.request_create_session(pending.command_id, definition_id);
                } else if pending.kind == crate::persistence::PendingKind::CancelTurn {
                    if let (Some(session_id), Some(turn_id), Some(position)) = (
                        pending.session_id,
                        pending.turn_id,
                        pending.requested_through_position,
                    ) {
                        state.request_cancel_turn(
                            pending.command_id,
                            session_id,
                            turn_id,
                            position,
                        );
                    }
                } else if pending.kind == crate::persistence::PendingKind::ContinueTurn {
                    let input_json = pending.request_payload.get("input_json").cloned();
                    if let (
                        Some(session_id),
                        Some(turn_id),
                        Some(suspension_id),
                        Some(version),
                        Some(schema_digest),
                        Some(input_json),
                    ) = (
                        pending.session_id,
                        pending.turn_id,
                        pending.suspension_id,
                        pending.expected_session_version,
                        deferred_schema_digest,
                        input_json,
                    ) {
                        state.request_continue_turn(
                            pending.command_id,
                            session_id,
                            turn_id,
                            suspension_id,
                            version,
                            schema_digest,
                            input_json,
                        );
                    }
                } else if state.admit_pending(pending.clone()) {
                    super::replay_pending(state, pending);
                }
            }
        }
        ActionOverlayIntent::ExactRetry => retry_pending(state),
        ActionOverlayIntent::OpenAbandonConfirmation => {
            state.model.overlay = Some(Overlay::AbandonConfirmation)
        }
        ActionOverlayIntent::ConfirmAbandon => state.abandon_pending(),
        ActionOverlayIntent::ReturnToUnknown => state.model.overlay = Some(Overlay::UnknownCommand),
        ActionOverlayIntent::SubmitSuspension => submit_suspension_response(state),
        ActionOverlayIntent::LeaveSafely => {
            state.model.return_overlay = Some(Overlay::Suspension);
            state.model.overlay = Some(Overlay::QuitConfirmation);
        }
    }
}

fn suspension_uses_editor(state: &RuntimeState) -> bool {
    state
        .model
        .suspension
        .as_ref()
        .and_then(|suspension| suspension.response_schema_json.as_deref())
        .and_then(response_schema_control)
        == Some(SchemaControl::Editor)
}

pub(super) fn move_suspension_choice(state: &mut RuntimeState, backwards: bool) {
    let count = state
        .model
        .suspension
        .as_ref()
        .and_then(|suspension| suspension.response_schema_json.as_deref())
        .and_then(response_schema_control)
        .and_then(|control| match control {
            SchemaControl::Choices(choices) => Some(choices.len()),
            SchemaControl::Editor => None,
        })
        .unwrap_or(0);
    let Some(response) = state.model.suspension_response.as_mut() else {
        return;
    };
    response.choice_selection = if backwards {
        response
            .choice_selection
            .checked_sub(1)
            .unwrap_or(count.saturating_sub(1))
    } else if count == 0 {
        0
    } else {
        (response.choice_selection + 1) % count
    };
}

fn action_overlay_key(key: KeyEvent) -> Option<ActionOverlayKey> {
    if key.code == KeyCode::Char('q')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
    {
        return Some(ActionOverlayKey::CtrlQ);
    }
    let plain = key.modifiers.is_empty();
    match key.code {
        KeyCode::Enter if plain => Some(ActionOverlayKey::Enter),
        KeyCode::Esc if plain => Some(ActionOverlayKey::Escape),
        KeyCode::Char(character)
            if plain || key.modifiers == crossterm::event::KeyModifiers::SHIFT =>
        {
            Some(ActionOverlayKey::Character(character.to_ascii_lowercase()))
        }
        _ => None,
    }
}
