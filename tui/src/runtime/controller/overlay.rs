use crossterm::event::{KeyCode, KeyEvent};

use crate::application::{ActionOverlayIntent, ActionOverlayKey, AppAction, Overlay};

use super::{
    actions::retry_pending,
    navigation::{
        is_safe_query_character, matching_commands, matching_history, matching_sessions,
        select_command, select_history, select_session,
    },
    RuntimeState,
};

pub(super) fn handle(key: KeyEvent, state: &mut RuntimeState) -> bool {
    let Some(overlay) = state.model.overlay else {
        return false;
    };
    if let Some(intent) =
        action_overlay_key(key.code).and_then(|normalized| overlay.action_for_key(normalized))
    {
        match intent {
            ActionOverlayIntent::Close => state.dispatch(AppAction::OverlayClosed),
            ActionOverlayIntent::ConfirmQuit => state.dispatch(AppAction::QuitConfirmed),
            ActionOverlayIntent::AcceptEphemeral => {
                state.ephemeral_confirmed = true;
                state.model.overlay = None;
            }
            ActionOverlayIntent::ExactRetry => retry_pending(state),
            ActionOverlayIntent::AbandonPending => state.abandon_pending(),
        }
        return true;
    }
    match key.code {
        KeyCode::Esc if overlay != Overlay::UnknownCommand => {
            state.dispatch(AppAction::OverlayClosed)
        }
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
            if overlay == Overlay::PromptHistory && is_safe_query_character(character) =>
        {
            state.model.history_filter.push(character);
            state.model.history_selection = 0;
        }
        KeyCode::Backspace if overlay == Overlay::PromptHistory => {
            state.model.history_filter.pop();
            state.model.history_selection = 0;
        }
        KeyCode::Enter if overlay == Overlay::Suspension => {
            state.editing_suspension = state
                .model
                .suspension
                .as_ref()
                .map(|value| value.suspension_id.clone());
            state.model.overlay = None;
        }
        _ => {}
    }
    true
}

fn action_overlay_key(key: KeyCode) -> Option<ActionOverlayKey> {
    match key {
        KeyCode::Enter => Some(ActionOverlayKey::Enter),
        KeyCode::Esc => Some(ActionOverlayKey::Escape),
        KeyCode::Char(character) => Some(ActionOverlayKey::Character(character)),
        _ => None,
    }
}
