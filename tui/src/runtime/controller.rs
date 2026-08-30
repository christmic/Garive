use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::Duration;

use crate::application::{AppAction, ExecutionState, FocusTarget, Overlay, TerminalSize};

use super::app::RuntimeState;

mod actions;
mod navigation;

pub(super) use actions::replay_pending;
use actions::{cancel, create_session, retry_pending, submit};
use navigation::{
    activate_navigation_selection, conversation_page_cells, cycle_focus, cycle_session_selection,
    is_safe_query_character, matching_commands, matching_history, matching_sessions,
    move_navigation_selection, move_navigation_to_edge, open_command_palette, open_prompt_history,
    open_session_picker, select_command, select_history, select_session,
};

pub(super) fn handle_terminal(event: Event, state: &mut RuntimeState) {
    match event {
        Event::Resize(width, height) => {
            state.dispatch(AppAction::TerminalResized(TerminalSize { width, height }))
        }
        Event::FocusGained => state.dispatch(AppAction::TerminalFocusChanged(true)),
        Event::FocusLost => state.dispatch(AppAction::TerminalFocusChanged(false)),
        Event::Paste(text) => {
            if state.composer_is_frozen() {
                state.explain_frozen_composer();
            } else {
                let _ = state.model.composer.insert(&text);
            }
        }
        Event::Mouse(mouse) => handle_mouse(mouse, state),
        Event::Key(key) if key.kind != KeyEventKind::Release => handle_key(key, state),
        _ => {}
    }
}

fn handle_mouse(mouse: MouseEvent, state: &mut RuntimeState) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            state.model.scroll_conversation_up(3)
        }
        MouseEventKind::ScrollDown => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            state.model.scroll_conversation_down(3)
        }
        MouseEventKind::Down(MouseButton::Left)
            if state.model.terminal_size.width >= 100
                && mouse.column
                    < if state.model.terminal_size.width >= 160 {
                        34
                    } else {
                        28
                    }
                && mouse.row >= 3 =>
        {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Navigation));
            let index = ((mouse.row - 3) / 3) as usize;
            if let Some(session) = state.model.sessions.get(index) {
                state.model.session_selection = index;
                state.model.navigation_selection = Some(session.session_id.clone());
                state.load(session.session_id.clone());
            }
        }
        _ => {}
    }
}

fn handle_key(key: KeyEvent, state: &mut RuntimeState) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
        state.dispatch(AppAction::QuitRequested);
        return;
    }
    if let Some(overlay) = state.model.overlay {
        match key.code {
            KeyCode::Esc if overlay != Overlay::UnknownCommand => {
                state.dispatch(AppAction::OverlayClosed)
            }
            KeyCode::Enter if overlay == Overlay::QuitConfirmation => {
                state.dispatch(AppAction::QuitConfirmed)
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
                cycle_session_selection(state, false)
            }
            KeyCode::BackTab if overlay == Overlay::SessionPicker => {
                cycle_session_selection(state, true)
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
            KeyCode::Enter if overlay == Overlay::EphemeralConfirmation => {
                state.ephemeral_confirmed = true;
                state.model.overlay = None;
            }
            KeyCode::Enter if overlay == Overlay::UnknownCommand => retry_pending(state),
            KeyCode::Char('a') if overlay == Overlay::UnknownCommand => state.abandon_pending(),
            _ => {}
        }
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => handle_ctrl_c(state),
            KeyCode::Char('n') => {
                create_session(state);
            }
            KeyCode::Char('s') => open_session_picker(state),
            KeyCode::Char('p') => open_command_palette(state),
            KeyCode::Char('r') => open_prompt_history(state),
            KeyCode::Char('j') => {
                if state.composer_is_frozen() {
                    state.explain_frozen_composer();
                } else {
                    let _ = state.model.composer.insert("\n");
                }
            }
            KeyCode::Char('l') if state.model.focus == FocusTarget::Conversation => {
                state.force_redraw = true;
            }
            KeyCode::Home if state.model.focus == FocusTarget::Conversation => {
                state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
                state.model.jump_to_oldest();
            }
            KeyCode::End if state.model.focus == FocusTarget::Conversation => {
                state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
                state.model.follow_latest();
            }
            KeyCode::Home => state
                .model
                .composer
                .move_document_start(key.modifiers.contains(KeyModifiers::SHIFT)),
            KeyCode::End => state
                .model
                .composer
                .move_document_end(key.modifiers.contains(KeyModifiers::SHIFT)),
            KeyCode::Char('z') => {
                if state.composer_is_frozen() {
                    state.explain_frozen_composer();
                } else {
                    state.model.composer.undo();
                }
            }
            KeyCode::Char('y') => {
                if state.composer_is_frozen() {
                    state.explain_frozen_composer();
                } else {
                    state.model.composer.redo();
                }
            }
            _ => {}
        }
        return;
    }
    if key.code == KeyCode::Char('?')
        && (state.model.composer.text().is_empty() || state.composer_is_frozen())
    {
        state.dispatch(AppAction::OverlayOpened(Overlay::Help));
        return;
    }
    if state.model.focus == FocusTarget::Navigation {
        match key.code {
            KeyCode::Up => move_navigation_selection(&mut state.model, true),
            KeyCode::Down => move_navigation_selection(&mut state.model, false),
            KeyCode::Home => move_navigation_to_edge(&mut state.model, false),
            KeyCode::End => move_navigation_to_edge(&mut state.model, true),
            KeyCode::Enter => activate_navigation_selection(state),
            _ => {}
        }
        if matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End | KeyCode::Enter
        ) {
            return;
        }
    }
    if state.composer_is_frozen()
        && matches!(
            key.code,
            KeyCode::Char(_)
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Enter
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
        )
    {
        state.explain_frozen_composer();
        return;
    }
    match key.code {
        KeyCode::Tab => cycle_focus(state, key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::BackTab => cycle_focus(state, true),
        KeyCode::Char(character) => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Composer));
            let _ = state.model.composer.insert(&character.to_string());
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
            state.model.composer.delete_word_left();
        }
        KeyCode::Delete if key.modifiers.contains(KeyModifiers::ALT) => {
            state.model.composer.delete_word_right();
        }
        KeyCode::Backspace => {
            state.model.composer.backspace();
        }
        KeyCode::Delete => {
            state.model.composer.delete();
        }
        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => state
            .model
            .composer
            .move_word_left(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => state
            .model
            .composer
            .move_word_right(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Left => state
            .model
            .composer
            .move_left(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Right => state
            .model
            .composer
            .move_right(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Up => state
            .model
            .composer
            .move_up(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Down => state
            .model
            .composer
            .move_down(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Home => state
            .model
            .composer
            .move_line_start(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::End if !key.modifiers.contains(KeyModifiers::CONTROL) => state
            .model
            .composer
            .move_line_end(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            let _ = state.model.composer.insert("\n");
        }
        KeyCode::Enter => submit(state),
        KeyCode::PageUp => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            state
                .model
                .scroll_conversation_up(conversation_page_cells(state));
        }
        KeyCode::PageDown => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            state
                .model
                .scroll_conversation_down(conversation_page_cells(state));
        }
        KeyCode::End if state.model.focus == FocusTarget::Conversation => {
            state.model.follow_latest()
        }
        KeyCode::Esc if state.model.execution == ExecutionState::Following => cancel(state),
        _ => {}
    }
}

fn handle_ctrl_c(state: &mut RuntimeState) {
    if state.model.execution == ExecutionState::Following {
        cancel(state);
        return;
    }
    if state.composer_is_frozen() {
        state.explain_frozen_composer();
        return;
    }
    if state.model.composer.has_selection() {
        state.model.composer.clear_selection();
        state.last_empty_ctrl_c = None;
    } else if !state.model.composer.text().is_empty() {
        state.model.composer.clear();
        state.last_empty_ctrl_c = None;
    } else {
        let now = std::time::Instant::now();
        if state
            .last_empty_ctrl_c
            .is_some_and(|previous| now.duration_since(previous) <= Duration::from_millis(1_500))
        {
            state.last_empty_ctrl_c = None;
            state.dispatch(AppAction::QuitRequested);
        } else {
            state.last_empty_ctrl_c = Some(now);
            state.model.notice = Some("Press Ctrl+C again to quit.".into());
        }
    }
}
