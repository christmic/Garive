use serde_json::{json, Value};

use crate::{
    application::{AppAction, ExecutionState, InspectorVariant, Overlay},
    input::{parse_command, parse_schema_input, Command, CommandParse, InspectorCommand},
    persistence::{now, DiagnosticEvent, PendingCommand, PendingKind},
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
        Command::Inspect(command) => set_inspector(command, state),
        Command::Help => state.model.overlay = Some(Overlay::Help),
        Command::Status => set_inspector(Some(InspectorCommand::Details), state),
        Command::EditPrompt => external_editor::request(state),
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

fn set_inspector(command: Option<InspectorCommand>, state: &mut RuntimeState) {
    if command == Some(InspectorCommand::Close) {
        state.model.inspector.open = false;
        state.model.inspector.selected_key = None;
        return;
    }
    let variant = match command {
        Some(InspectorCommand::Activity) => InspectorVariant::Activity,
        Some(InspectorCommand::Recovery) => InspectorVariant::Recovery,
        Some(InspectorCommand::Details) => InspectorVariant::Details,
        Some(InspectorCommand::Close) => return,
        None => state.model.default_inspector_variant(),
    };
    state.model.inspector.open = true;
    state.model.select_inspector_variant(variant);
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
