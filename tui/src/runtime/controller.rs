use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    application::{ExecutionState, Overlay, TerminalSize},
    input::{parse_command, Command, CommandParse},
};

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
            KeyCode::Char('r') => state.model.overlay = Some(Overlay::PromptHistory),
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
        KeyCode::PageUp => state.model.scroll_offset = state.model.scroll_offset.saturating_sub(5),
        KeyCode::PageDown | KeyCode::End => {
            state.model.scroll_offset = state.model.timeline.len().saturating_sub(1)
        }
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
    create_session_with(state, None);
}

fn create_session_with(state: &mut RuntimeState, requested: Option<String>) {
    let definition = requested
        .or_else(|| state.config.definition.clone())
        .or_else(|| {
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
    match parse_command(&text) {
        CommandParse::Valid(command) => {
            state.model.composer.clear();
            execute_command(command, state);
            return;
        }
        CommandParse::Invalid => {
            state.model.notice = Some("The slash command is invalid; nothing was sent.".into());
            state.model.overlay = Some(Overlay::UnknownCommand);
            return;
        }
        CommandParse::NotCommand => {}
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

fn execute_command(command: Command, state: &mut RuntimeState) {
    match command {
        Command::New { definition } => create_session_with(state, definition),
        Command::Sessions { filter } => {
            state.model.session_filter = filter.unwrap_or_default();
            state.model.overlay = Some(Overlay::SessionPicker);
        }
        Command::Help => state.model.overlay = Some(Overlay::Help),
        Command::Status => {
            state.model.notice = Some(format!(
                "Host: {}\nSession: {}\nCursor: {}",
                match state.model.connection {
                    crate::application::ConnectionState::Online => "online",
                    _ => "not online",
                },
                state.model.selected_session.as_deref().unwrap_or("none"),
                state.model.observed_position
            ));
            state.model.overlay = Some(Overlay::ErrorDetails);
        }
        Command::Reconnect => {
            if let Some(session) = state.model.selected_session.clone() {
                state.load(session);
            }
        }
        Command::Cancel => cancel(state),
        Command::Theme(theme) => state.config.theme = theme,
        Command::Mouse(mouse) => {
            state.config.mouse = mouse;
            state.model.notice =
                Some("Mouse preference updated for the next terminal session.".into());
            state.model.overlay = Some(Overlay::ErrorDetails);
        }
        Command::Retry => {
            state.model.notice = Some("No recoverable pending command is loaded.".into());
            state.model.overlay = Some(Overlay::ErrorDetails);
        }
        Command::CopyLast | Command::CopySessionId => {
            state.model.notice =
                Some("Clipboard integration is unavailable in this terminal.".into());
            state.model.overlay = Some(Overlay::ErrorDetails);
        }
        Command::Quit => state.model.overlay = Some(Overlay::QuitConfirmation),
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
