use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::time::Duration;

use crate::{
    application::{AppAction, ExecutionState, FocusTarget, Overlay, TerminalSize},
    input::{
        command_matches, parse_command, parse_schema_input, Command, CommandParse, COMMAND_PALETTE,
    },
    persistence::{now, DiagnosticEvent, PendingCommand, PendingKind},
};
use serde_json::{json, Value};

use super::{app::RuntimeState, clipboard, host};

pub(super) fn handle_terminal(event: Event, state: &mut RuntimeState) {
    match event {
        Event::Resize(width, height) => {
            state.dispatch(AppAction::TerminalResized(TerminalSize { width, height }))
        }
        Event::FocusGained => state.dispatch(AppAction::TerminalFocusChanged(true)),
        Event::FocusLost => state.dispatch(AppAction::TerminalFocusChanged(false)),
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
                let _ = state.model.composer.insert("\n");
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
        KeyCode::Tab => cycle_focus(state, key.modifiers.contains(KeyModifiers::SHIFT)),
        KeyCode::BackTab => cycle_focus(state, true),
        KeyCode::Char('?') if state.model.composer.text().is_empty() => {
            state.dispatch(AppAction::OverlayOpened(Overlay::Help))
        }
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

fn cycle_focus(state: &mut RuntimeState, backwards: bool) {
    let next = next_focus(
        state.model.terminal_size.width,
        state.model.focus,
        backwards,
    );
    state.dispatch(AppAction::FocusChanged(next));
}

fn next_focus(width: u16, current: FocusTarget, backwards: bool) -> FocusTarget {
    match (width >= 100, backwards, current) {
        (true, false, FocusTarget::Navigation) => FocusTarget::Conversation,
        (true, false, FocusTarget::Conversation) => FocusTarget::Composer,
        (true, false, _) => FocusTarget::Navigation,
        (true, true, FocusTarget::Navigation) => FocusTarget::Composer,
        (true, true, FocusTarget::Conversation) => FocusTarget::Navigation,
        (true, true, _) => FocusTarget::Conversation,
        (false, false, FocusTarget::Conversation) => FocusTarget::Composer,
        (false, false, _) => FocusTarget::Conversation,
        (false, true, FocusTarget::Composer) => FocusTarget::Conversation,
        (false, true, _) => FocusTarget::Composer,
    }
}

fn handle_ctrl_c(state: &mut RuntimeState) {
    if state.model.execution == ExecutionState::Following {
        cancel(state);
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

fn conversation_page_cells(state: &RuntimeState) -> usize {
    usize::from(state.model.terminal_size.height.saturating_sub(8) / 3).max(1)
}

fn is_safe_query_character(character: char) -> bool {
    !character.is_control()
        && !matches!(
            character,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        )
}

fn select_command(state: &mut RuntimeState) {
    let Some(index) = matching_commands(state)
        .get(state.model.command_selection)
        .copied()
    else {
        return;
    };
    let name = COMMAND_PALETTE[index].0;
    if let Some(reason) = command_disabled_reason(name, state) {
        state.model.notice = Some(format!("Command unavailable: {reason}."));
        return;
    }
    state.model.overlay = None;
    if let CommandParse::Valid(command) = parse_command(name) {
        execute_command(command, state);
    }
}

fn select_history(state: &mut RuntimeState) {
    if let Some(text) = matching_history(state)
        .get(state.model.history_selection)
        .cloned()
    {
        let _ = state.model.composer.replace(&text);
    }
    state.model.overlay = None;
}

fn open_command_palette(state: &mut RuntimeState) {
    state.model.command_filter.clear();
    state.model.command_selection = 0;
    state.dispatch(AppAction::OverlayOpened(Overlay::CommandPalette));
}

fn open_prompt_history(state: &mut RuntimeState) {
    state.model.history_filter.clear();
    state.model.history_selection = 0;
    state.dispatch(AppAction::OverlayOpened(Overlay::PromptHistory));
}

fn matching_commands(state: &RuntimeState) -> Vec<usize> {
    COMMAND_PALETTE
        .iter()
        .enumerate()
        .filter(|(_, (name, help))| command_matches(name, help, &state.model.command_filter))
        .map(|(index, _)| index)
        .collect()
}

fn matching_history(state: &RuntimeState) -> Vec<String> {
    let filter = state.model.history_filter.to_lowercase();
    state
        .model
        .prompt_history
        .iter()
        .filter(|text| filter.is_empty() || text.to_lowercase().contains(&filter))
        .cloned()
        .collect()
}

fn command_disabled_reason(name: &str, state: &RuntimeState) -> Option<&'static str> {
    match name {
        "/new" if state.model.definitions.is_empty() => Some("no Agent is installed"),
        "/retry" if state.pending_for_context().is_none() => Some("no pending command"),
        "/cancel" if state.model.execution != ExecutionState::Following => {
            Some("no Turn is running")
        }
        "/copy last"
            if !state
                .model
                .timeline
                .iter()
                .any(|item| item.role == crate::application::TimelineRole::Agent) =>
        {
            Some("no completion is visible")
        }
        "/copy session-id" if state.model.selected_session.is_none() => Some("no Session selected"),
        _ => None,
    }
}

fn select_session(state: &mut RuntimeState) {
    let selected = matching_sessions(state)
        .get(state.model.session_selection)
        .cloned();
    state.model.overlay = None;
    if let Some(id) = selected {
        state.load(id);
    }
}

fn open_session_picker(state: &mut RuntimeState) {
    state.model.session_filter.clear();
    state.model.session_selection = state
        .model
        .selected_session
        .as_deref()
        .and_then(|selected| {
            state
                .model
                .sessions
                .iter()
                .position(|session| session.session_id == selected)
        })
        .unwrap_or(0);
    state.dispatch(AppAction::OverlayOpened(Overlay::SessionPicker));
}

fn cycle_session_selection(state: &mut RuntimeState, backwards: bool) {
    let count = matching_sessions(state).len();
    if count == 0 {
        return;
    }
    state.model.session_selection = if backwards {
        state
            .model
            .session_selection
            .checked_sub(1)
            .unwrap_or(count - 1)
    } else {
        (state.model.session_selection + 1) % count
    };
}

fn matching_sessions(state: &RuntimeState) -> Vec<String> {
    let filter = state.model.session_filter.to_lowercase();
    state
        .model
        .sessions
        .iter()
        .filter(|session| filter.is_empty() || session.session_id.to_lowercase().contains(&filter))
        .map(|session| session.session_id.clone())
        .collect()
}

fn create_session(state: &mut RuntimeState) -> bool {
    create_session_with(state, None)
}

fn create_session_with(state: &mut RuntimeState, requested: Option<String>) -> bool {
    if requested.is_none() && state.config.definition.is_none() && state.model.definitions.len() > 1
    {
        state.model.notice = Some("Choose an Agent with /new <definition-id>.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        return false;
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
            return false;
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
            return false;
        }
        host::create_session(state.client.clone(), id, definition, state.sender.clone());
        true
    } else {
        state.model.notice = Some("No Agent definition is installed yet.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        false
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
        state.queued_prompt = Some(text);
        if !create_session(state) {
            state.queued_prompt = None;
        }
        return;
    }
    let session = state.model.selected_session.clone().unwrap_or_default();
    let id = state.command_id("turn");
    if let (Some(turn), Some(suspension)) = (
        state.model.selected_turn.clone(),
        state.model.suspension.clone(),
    ) {
        if suspension.response_schema_json.is_some() {
            let Some(schema) = suspension.response_schema_json.as_deref() else {
                return;
            };
            let Ok(input_json) = parse_schema_input(schema, &text) else {
                state.model.notice =
                    Some("The response does not match the public response schema.".into());
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
            host::continue_turn(
                state.client.clone(),
                host::ContinuationRequest {
                    command_id: id,
                    session_id: session,
                    turn_id: turn,
                    suspension_id: suspension.suspension_id,
                    expected_session_version: suspension.session_version,
                    input: host::ContinuationInput::Json(input_json),
                },
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
                host::ContinuationRequest {
                    command_id: id,
                    session_id: session,
                    turn_id: turn,
                    suspension_id: suspension.suspension_id,
                    expected_session_version: suspension.session_version,
                    input: host::ContinuationInput::Text(text),
                },
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
        Command::New { definition } => {
            create_session_with(state, definition);
        }
        Command::Sessions { filter } => {
            state.model.session_filter = filter.unwrap_or_default();
            state.model.session_selection = 0;
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
        Command::CopyLast => {
            let value = state
                .model
                .timeline
                .iter()
                .rev()
                .find(|item| item.role == crate::application::TimelineRole::Agent)
                .map(|item| item.text.clone());
            copy_value(value, state);
        }
        Command::CopySessionId => {
            copy_value(state.model.selected_session.clone(), state);
        }
        Command::Quit => state.dispatch(AppAction::QuitRequested),
    }
}

fn copy_value(value: Option<String>, state: &mut RuntimeState) {
    let notice = if state.config.screen_reader {
        "Clipboard requests are disabled in screen-reader mode."
    } else if let Some(value) = value {
        if clipboard::copy(&value).is_ok() {
            "Copy request sent to the terminal."
        } else {
            "The terminal rejected the bounded copy request."
        }
    } else {
        "There is no visible value to copy."
    };
    state.model.notice = Some(notice.into());
    state.model.overlay = Some(Overlay::ErrorDetails);
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
    let Some(pending) = state.pending_for_context().cloned() else {
        state.model.notice = Some("No recoverable pending command is loaded.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        return;
    };
    if pending.validate().is_err() {
        state.model.notice =
            Some("The pending command digest is invalid; retry was blocked.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        return;
    }
    state.retry_after_refresh = Some(pending.command_id.clone());
    let _ = state.store.record_diagnostic(DiagnosticEvent::RetryQueued);
    state.model.overlay = None;
    state.model.notice = Some("Refreshing Host truth before exact retry…".into());
    if let Some(session_id) = pending.session_id {
        state.load(session_id);
    } else {
        host::bootstrap(state.client.clone(), state.sender.clone());
    }
}

pub(super) fn replay_pending(state: &mut RuntimeState, pending: PendingCommand) {
    let _ = state.store.record_diagnostic(DiagnosticEvent::RetrySent);
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
            host::ContinuationRequest {
                command_id: pending.command_id,
                session_id: session,
                turn_id: turn,
                suspension_id: suspension,
                expected_session_version: version,
                input: host::ContinuationInput::Text(input.into()),
            },
            state.sender.clone(),
        );
    } else if let Some(input) = pending.request_payload.get("input_json") {
        host::continue_turn(
            state.client.clone(),
            host::ContinuationRequest {
                command_id: pending.command_id,
                session_id: session,
                turn_id: turn,
                suspension_id: suspension,
                expected_session_version: version,
                input: host::ContinuationInput::Json(input.clone()),
            },
            state.sender.clone(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycle_matches_responsive_regions() {
        assert_eq!(
            next_focus(120, FocusTarget::Composer, false),
            FocusTarget::Navigation
        );
        assert_eq!(
            next_focus(120, FocusTarget::Navigation, false),
            FocusTarget::Conversation
        );
        assert_eq!(
            next_focus(80, FocusTarget::Composer, false),
            FocusTarget::Conversation
        );
        assert_eq!(
            next_focus(80, FocusTarget::Conversation, true),
            FocusTarget::Composer
        );
    }
}
