use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::application::{ExecutionState, Overlay, TerminalSize};

use super::{app::RuntimeState, host};

pub(super) fn handle_terminal(event: Event, state: &mut RuntimeState) {
    match event {
        Event::Resize(width, height) => state.model.terminal_size = TerminalSize { width, height },
        Event::FocusGained => state.model.terminal_focused = true,
        Event::FocusLost => state.model.terminal_focused = false,
        Event::Paste(text) => {
            let _ = state.model.composer.insert(&text);
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => handle_key(key, state),
        _ => {}
    }
}

fn handle_key(key: KeyEvent, state: &mut RuntimeState) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
        state.model.overlay = Some(Overlay::QuitConfirmation);
        return;
    }
    if let Some(overlay) = state.model.overlay {
        match key.code {
            KeyCode::Esc if overlay != Overlay::Suspension => state.model.overlay = None,
            KeyCode::Enter if overlay == Overlay::QuitConfirmation => {
                state.model.quit_requested = true
            }
            KeyCode::Up if overlay == Overlay::SessionPicker => {
                state.model.session_selection = state.model.session_selection.saturating_sub(1)
            }
            KeyCode::Down if overlay == Overlay::SessionPicker => {
                state.model.session_selection = (state.model.session_selection + 1)
                    .min(state.model.sessions.len().saturating_sub(1))
            }
            KeyCode::Enter if overlay == Overlay::SessionPicker => select_session(state),
            KeyCode::Enter if overlay == Overlay::Suspension => {
                state.model.overlay = None;
            }
            _ => {}
        }
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('n') => create_session(state),
            KeyCode::Char('s') => state.model.overlay = Some(Overlay::SessionPicker),
            KeyCode::Char('p') => state.model.overlay = Some(Overlay::CommandPalette),
            KeyCode::Char('j') => {
                let _ = state.model.composer.insert("\n");
            }
            KeyCode::Char('z') => {
                state.model.composer.undo();
            }
            KeyCode::Char('y') => {
                state.model.composer.redo();
            }
            _ => {}
        }
        return;
    }
    match key.code {
        KeyCode::Char('?') if state.model.composer.text().is_empty() => {
            state.model.overlay = Some(Overlay::Help)
        }
        KeyCode::Char(character) => {
            let _ = state.model.composer.insert(&character.to_string());
        }
        KeyCode::Backspace => {
            state.model.composer.backspace();
        }
        KeyCode::Delete => {
            state.model.composer.delete();
        }
        KeyCode::Left => state
            .model
            .composer
            .move_left(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Right => state
            .model
            .composer
            .move_right(key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::Enter => submit(state),
        KeyCode::Esc if state.model.execution == ExecutionState::Following => cancel(state),
        _ => {}
    }
}

fn select_session(state: &mut RuntimeState) {
    let selected = state
        .model
        .sessions
        .get(state.model.session_selection)
        .map(|item| item.session_id.clone());
    state.model.overlay = None;
    if let Some(id) = selected {
        state.load(id);
    }
}

fn create_session(state: &mut RuntimeState) {
    let definition = state.config.definition.clone().or_else(|| {
        state
            .model
            .definitions
            .first()
            .map(|item| item.definition_id.clone())
    });
    if let Some(definition) = definition {
        let id = state.command_id("create");
        host::create_session(state.client.clone(), id, definition, state.sender.clone());
    }
}

fn submit(state: &mut RuntimeState) {
    let text = state.model.composer.text().trim().to_owned();
    if text.is_empty() {
        return;
    }
    if state.model.selected_session.is_none() {
        create_session(state);
        return;
    }
    let session = state.model.selected_session.clone().unwrap_or_default();
    let id = state.command_id("turn");
    if let (Some(turn), Some(suspension)) = (
        state.model.selected_turn.clone(),
        state.model.suspension.clone(),
    ) {
        host::continue_turn(
            state.client.clone(),
            id,
            session,
            turn,
            suspension.suspension_id,
            suspension.session_version,
            text,
            state.sender.clone(),
        );
    } else if matches!(
        state.model.execution,
        ExecutionState::Idle | ExecutionState::Failed
    ) {
        host::start_turn(
            state.client.clone(),
            id,
            session,
            text,
            state.sender.clone(),
        );
    }
}

fn cancel(state: &mut RuntimeState) {
    if let (Some(session), Some(turn)) = (
        state.model.selected_session.clone(),
        state.model.selected_turn.clone(),
    ) {
        let id = state.command_id("cancel");
        host::cancel_turn(
            state.client.clone(),
            id,
            session,
            turn,
            state.model.observed_position.max(1),
            state.sender.clone(),
        );
    }
}
