use serde_json::Value;

use crate::{
    application::{AppAction, ExecutionState, Overlay},
    input::{
        parse_command, parse_schema_input, response_schema_control, Command, CommandParse,
        InspectorCommand, SchemaControl,
    },
    persistence::{DiagnosticEvent, PendingCommand, PendingKind},
};

use super::{
    super::{clipboard, external_editor, host},
    RuntimeState,
};

pub(super) fn create_session(state: &mut RuntimeState) -> bool {
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
        if !state.request_create_session(id, definition) {
            return false;
        }
        true
    } else {
        state.model.notice = Some("No Agent definition is installed yet.".into());
        state.model.overlay = Some(Overlay::ErrorDetails);
        false
    }
}

pub(super) fn submit(state: &mut RuntimeState) {
    let text = state.model.composer.text().trim().to_owned();
    if text.is_empty() {
        return;
    }
    match parse_command(&text) {
        CommandParse::Valid(command) => {
            state.model.composer.clear();
            state.model.prompt_history_browser.reset();
            execute_command(command, state);
            return;
        }
        CommandParse::Invalid => {
            state.model.notice = Some("The slash command is invalid; nothing was sent.".into());
            state.model.overlay = Some(Overlay::ErrorDetails);
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
    if state.model.suspension.is_some() {
        state.model.notice = Some(if state.model.suspension_is_interactive() {
            "This Turn is waiting for a typed response; the ordinary draft was not sent.".into()
        } else {
            "This suspension is read-only; the ordinary draft was not sent.".into()
        });
        state.model.overlay = Some(Overlay::Suspension);
        return;
    }
    let session = state.model.selected_session.clone().unwrap_or_default();
    let id = state.command_id("turn");
    if matches!(
        state.model.execution,
        ExecutionState::Idle | ExecutionState::Failed
    ) {
        state.request_start_turn(id, session, text);
    }
}

pub(super) fn submit_suspension_response(state: &mut RuntimeState) {
    let (Some(response), Some(suspension), Some(session), Some(turn)) = (
        state.model.suspension_response.as_ref(),
        state.model.suspension.as_ref(),
        state.model.selected_session.as_ref(),
        state.model.selected_turn.as_ref(),
    ) else {
        state.model.notice = Some("This suspension is read-only.".into());
        return;
    };
    let schema_digest = suspension
        .response_schema_digest
        .as_deref()
        .unwrap_or_default()
        .to_owned();
    if response.identity.session_id != *session
        || response.identity.turn_id != *turn
        || response.identity.suspension_id != suspension.suspension_id
        || response.identity.schema_digest != schema_digest
    {
        state.model.suspension_response = None;
        state.model.notice =
            Some("The suspension changed; the stale response was discarded.".into());
        return;
    }
    let Some(schema) = suspension.response_schema_json.as_deref() else {
        return;
    };
    let raw = match response_schema_control(schema) {
        Some(SchemaControl::Editor) => response.editor.text().to_owned(),
        Some(SchemaControl::Choices(choices)) => choices
            .get(response.choice_selection)
            .cloned()
            .unwrap_or_default(),
        None => return,
    };
    let Ok(input_json) = parse_schema_input(schema, &raw) else {
        state.model.notice = Some("The response does not match the public response schema.".into());
        return;
    };
    let (session, turn, suspension_id, version) = (
        session.clone(),
        turn.clone(),
        suspension.suspension_id.clone(),
        suspension.session_version,
    );
    let id = state.command_id("continue");
    state.request_continue_turn(
        id,
        session,
        turn,
        suspension_id,
        version,
        schema_digest,
        input_json,
    );
}

pub(super) fn execute_command(command: Command, state: &mut RuntimeState) {
    match command {
        Command::New { definition } => {
            create_session_with(state, definition);
        }
        Command::Sessions { filter } => {
            state.model.session_filter = filter.unwrap_or_default();
            state.model.session_selection = 0;
            state.model.overlay = Some(Overlay::SessionPicker);
        }
        Command::Jump { filter } => super::navigation::open_turn_navigator(state, filter),
        Command::Inspect(command) => super::inspector::set_command(command, state),
        Command::Help => state.model.overlay = Some(Overlay::Help),
        Command::Status => super::inspector::set_command(Some(InspectorCommand::Details), state),
        Command::EditPrompt => external_editor::request(state),
        Command::Reconnect => {
            if let Some(session) = state.model.selected_session.clone() {
                state.load(session);
            }
        }
        Command::Cancel => cancel(state),
        Command::Theme(theme) => state.config.theme = theme,
        Command::Mouse(mouse) => state.set_mouse_mode(mouse),
        Command::Retry => retry_pending(state),
        Command::CopyLast => {
            let value = state
                .model
                .durable_children()
                .filter(|item| item.role == crate::application::TimelineRole::Agent)
                .last()
                .map(|item| item.text.clone());
            copy_value(value, state, true);
        }
        Command::CopySelection => {
            copy_composer_selection(state, true);
        }
        Command::CopySessionId => {
            copy_value(state.model.selected_session.clone(), state, true);
        }
        Command::Quit => state.dispatch(AppAction::QuitRequested),
    }
}

pub(super) fn copy_composer_selection(state: &mut RuntimeState, show_details: bool) {
    let value = state.model.composer.selected_text().map(str::to_owned);
    copy_value(value, state, show_details);
}

fn copy_value(value: Option<String>, state: &mut RuntimeState, show_details: bool) {
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
    if show_details {
        state.model.overlay = Some(Overlay::ErrorDetails);
    }
}

pub(super) fn cancel(state: &mut RuntimeState) {
    if let (Some(session), Some(turn)) = (
        state.model.selected_session.clone(),
        state.model.selected_turn.clone(),
    ) {
        let id = state.command_id("cancel");
        let position = state.model.observed_position.max(1);
        state.request_cancel_turn(id, session, turn, position);
    }
}

pub(super) fn retry_pending(state: &mut RuntimeState) {
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

pub(in crate::runtime) fn replay_pending(state: &mut RuntimeState, pending: PendingCommand) {
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
