use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::{
    application::{ExecutionState, Overlay, TerminalSize},
    input::{parse_command, Command, CommandParse},
    persistence::{now, PendingCommand, PendingKind},
};
use serde_json::{json, Value};

use super::{app::RuntimeState, host};

pub(super) fn handle_terminal(event: Event, state: &mut RuntimeState) {
    match event {
        Event::Resize(width, height) => state.model.terminal_size = TerminalSize { width, height },
        Event::FocusGained => state.model.terminal_focused = true,
        Event::FocusLost => state.model.terminal_focused = false,
        Event::Paste(text) => {
            let _ = state.model.composer.insert(&text);
        }
        Event::Mouse(mouse) => handle_mouse(mouse, state),
        Event::Key(key) if key.kind != KeyEventKind::Release => handle_key(key, state),
        _ => {}
    }
}

fn handle_mouse(mouse: MouseEvent, state: &mut RuntimeState) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            state.model.scroll_offset = state.model.scroll_offset.saturating_sub(3)
        }
        MouseEventKind::ScrollDown => {
            state.model.scroll_offset =
                (state.model.scroll_offset + 3).min(state.model.timeline.len().saturating_sub(1))
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
            let index = ((mouse.row - 3) / 3) as usize;
            if let Some(session) = state.model.sessions.get(index) {
                state.model.session_selection = index;
                state.load(session.session_id.clone());
            }
        }
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
            KeyCode::Esc if overlay != Overlay::UnknownCommand => state.model.overlay = None,
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
            KeyCode::Up if overlay == Overlay::PromptHistory => {
                state.model.history_selection = state.model.history_selection.saturating_sub(1)
            }
            KeyCode::Down if overlay == Overlay::PromptHistory => {
                state.model.history_selection = (state.model.history_selection + 1)
                    .min(state.model.prompt_history.len().saturating_sub(1))
            }
            KeyCode::Enter if overlay == Overlay::PromptHistory => select_history(state),
            KeyCode::Enter if overlay == Overlay::Suspension => {
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
        KeyCode::Enter => submit(state),
        KeyCode::PageUp => state.model.scroll_offset = state.model.scroll_offset.saturating_sub(5),
        KeyCode::PageDown | KeyCode::End => {
            state.model.scroll_offset = state.model.timeline.len().saturating_sub(1)
        }
        KeyCode::Esc if state.model.execution == ExecutionState::Following => cancel(state),
        _ => {}
    }
}

fn select_history(state: &mut RuntimeState) {
    if let Some(text) = state
        .model
        .prompt_history
        .get(state.model.history_selection)
        .cloned()
    {
        let _ = state.model.composer.replace(&text);
    }
    state.model.overlay = None;
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
    if requested.is_none() && state.config.definition.is_none() && state.model.definitions.len() > 1
    {
        state.model.notice = Some("Choose an Agent with /new <definition-id>.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        return;
    }
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
        if !state
            .model
            .definitions
            .iter()
            .any(|value| value.definition_id == definition)
        {
            state.model.notice = Some("That Agent definition is not installed.".into());
            state.model.overlay = Some(Overlay::ErrorDetails);
            return;
        }
        if !admit(
            state,
            id.clone(),
            PendingKind::CreateSession,
            None,
            None,
            None,
            None,
            None,
            json!({"agent_definition_id": definition}),
        ) {
            return;
        }
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
        if suspension.response_schema_json.is_some() {
            let Ok(input_json) = serde_json::from_str::<Value>(&text) else {
                state.model.notice = Some("This request expects a valid JSON response.".into());
                state.model.overlay = Some(Overlay::ErrorDetails);
                return;
            };
            if !admit(
                state,
                id.clone(),
                PendingKind::ContinueTurn,
                Some(session.clone()),
                Some(turn.clone()),
                Some(suspension.suspension_id.clone()),
                Some(suspension.session_version),
                None,
                json!({"input_json": input_json}),
            ) {
                return;
            }
            host::continue_turn_json(
                state.client.clone(),
                id,
                session,
                turn,
                suspension.suspension_id,
                suspension.session_version,
                input_json,
                state.sender.clone(),
            );
        } else {
            if !admit(
                state,
                id.clone(),
                PendingKind::ContinueTurn,
                Some(session.clone()),
                Some(turn.clone()),
                Some(suspension.suspension_id.clone()),
                Some(suspension.session_version),
                None,
                json!({"input": text}),
            ) {
                return;
            }
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
        }
    } else if matches!(
        state.model.execution,
        ExecutionState::Idle | ExecutionState::Failed
    ) {
        if !admit(
            state,
            id.clone(),
            PendingKind::StartTurn,
            Some(session.clone()),
            None,
            None,
            None,
            None,
            json!({"text": text}),
        ) {
            return;
        }
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
        Command::Retry => retry_pending(state),
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
        let position = state.model.observed_position.max(1);
        if !admit(
            state,
            id.clone(),
            PendingKind::CancelTurn,
            Some(session.clone()),
            Some(turn.clone()),
            None,
            None,
            Some(position),
            json!({"session_id": session, "requested_through_position": position}),
        ) {
            return;
        }
        host::cancel_turn(
            state.client.clone(),
            id,
            session,
            turn,
            position,
            state.sender.clone(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn admit(
    state: &mut RuntimeState,
    command_id: String,
    kind: PendingKind,
    session_id: Option<String>,
    turn_id: Option<String>,
    suspension_id: Option<String>,
    expected_session_version: Option<u64>,
    requested_through_position: Option<u64>,
    request_payload: Value,
) -> bool {
    state.admit_pending(PendingCommand {
        schema_version: 1,
        command_id,
        kind,
        session_id,
        turn_id,
        suspension_id,
        expected_session_version,
        requested_through_position,
        request_payload,
        request_digest: String::new(),
        created_at: now(),
    })
}

fn retry_pending(state: &mut RuntimeState) {
    let Some(pending) = state.pending.clone() else {
        state.model.notice = Some("No recoverable pending command is loaded.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        return;
    };
    let text = pending
        .request_payload
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match pending.kind {
        PendingKind::CreateSession => {
            if let Some(definition) = pending
                .request_payload
                .get("agent_definition_id")
                .and_then(Value::as_str)
            {
                host::create_session(
                    state.client.clone(),
                    pending.command_id,
                    definition.into(),
                    state.sender.clone(),
                );
            }
        }
        PendingKind::StartTurn => {
            if let Some(session) = pending.session_id {
                host::start_turn(
                    state.client.clone(),
                    pending.command_id,
                    session,
                    text,
                    state.sender.clone(),
                );
            }
        }
        PendingKind::CancelTurn => {
            if let (Some(session), Some(turn), Some(position)) = (
                pending.session_id,
                pending.turn_id,
                pending.requested_through_position,
            ) {
                host::cancel_turn(
                    state.client.clone(),
                    pending.command_id,
                    session,
                    turn,
                    position,
                    state.sender.clone(),
                );
            }
        }
        PendingKind::ContinueTurn => retry_continuation(state, pending),
    }
}

fn retry_continuation(state: &mut RuntimeState, pending: PendingCommand) {
    let (Some(session), Some(turn), Some(suspension), Some(version)) = (
        pending.session_id,
        pending.turn_id,
        pending.suspension_id,
        pending.expected_session_version,
    ) else {
        return;
    };
    if let Some(input) = pending.request_payload.get("input").and_then(Value::as_str) {
        host::continue_turn(
            state.client.clone(),
            pending.command_id,
            session,
            turn,
            suspension,
            version,
            input.into(),
            state.sender.clone(),
        );
    } else if let Some(input) = pending.request_payload.get("input_json") {
        host::continue_turn_json(
            state.client.clone(),
            pending.command_id,
            session,
            turn,
            suspension,
            version,
            input.clone(),
            state.sender.clone(),
        );
    }
}
