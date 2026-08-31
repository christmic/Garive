use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::application::{ActionOverlayIntent, ActionOverlayKey, AppAction, Overlay};

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
        match intent {
            ActionOverlayIntent::Close => state.dispatch(AppAction::OverlayClosed),
            ActionOverlayIntent::ConfirmQuit => state.dispatch(AppAction::QuitConfirmed),
            ActionOverlayIntent::AcceptEphemeral => {
                state.ephemeral_confirmed = true;
                state.model.overlay = None;
            }
            ActionOverlayIntent::ExactRetry => retry_pending(state),
            ActionOverlayIntent::OpenAbandonConfirmation => {
                state.model.overlay = Some(Overlay::AbandonConfirmation)
            }
            ActionOverlayIntent::ConfirmAbandon => state.abandon_pending(),
            ActionOverlayIntent::ReturnToUnknown => {
                state.model.overlay = Some(Overlay::UnknownCommand)
            }
            ActionOverlayIntent::SubmitSuspension => submit_suspension_response(state),
            ActionOverlayIntent::LeaveSafely => {
                state.model.return_overlay = Some(Overlay::Suspension);
                state.model.overlay = Some(Overlay::QuitConfirmation);
            }
        }
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
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            if let Some(response) = state.model.suspension_response.as_mut() {
                let _ = response.editor.insert(&character.to_string());
            }
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
