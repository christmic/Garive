use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    application::{AppAction, FocusTarget, InspectorActivation, InspectorVariant, Overlay},
    input::InspectorCommand,
};

use super::{actions::retry_pending, RuntimeState};

pub(super) fn set_command(command: Option<InspectorCommand>, state: &mut RuntimeState) {
    if command == Some(InspectorCommand::Close) {
        state.model.close_inspector();
        return;
    }
    let variant = match command {
        Some(InspectorCommand::Activity) => InspectorVariant::Activity,
        Some(InspectorCommand::Recovery) => InspectorVariant::Recovery,
        Some(InspectorCommand::Details) => InspectorVariant::Details,
        Some(InspectorCommand::Close) => return,
        None => state.model.default_inspector_variant(),
    };
    state.model.open_inspector(variant);
}

pub(super) fn handle_key(key: KeyEvent, state: &mut RuntimeState) -> bool {
    match key.code {
        KeyCode::Up => move_selection(state, true),
        KeyCode::Down => move_selection(state, false),
        KeyCode::Home => select_index(state, 0),
        KeyCode::End => {
            let last = state
                .model
                .inspector_projection()
                .entries
                .len()
                .saturating_sub(1);
            select_index(state, last);
        }
        KeyCode::Enter => activate(state),
        KeyCode::Esc => state.model.close_inspector(),
        KeyCode::Tab | KeyCode::BackTab => return false,
        _ => {}
    }
    true
}

pub(super) fn move_selection(state: &mut RuntimeState, backwards: bool) {
    let count = state.model.inspector_projection().entries.len();
    if count == 0 {
        return;
    }
    let current = state.model.inspector_selection();
    select_index(
        state,
        if backwards {
            current.saturating_sub(1)
        } else {
            current.saturating_add(1).min(count - 1)
        },
    );
}

pub(super) fn select_index(state: &mut RuntimeState, index: usize) {
    state.model.select_inspector_index(index);
}

pub(super) fn activate(state: &mut RuntimeState) {
    let activation = state
        .model
        .inspector_projection()
        .entries
        .get(state.model.inspector_selection())
        .map(|entry| entry.activation.clone())
        .unwrap_or(InspectorActivation::None);
    match activation {
        InspectorActivation::None => {}
        InspectorActivation::Turn { started_position } => {
            if state.model.jump_to_turn_position(started_position) {
                if state.model.terminal_size.width < 120 {
                    state.model.close_inspector();
                } else {
                    state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
                }
            }
        }
        InspectorActivation::RetryPending => {
            state.model.close_inspector();
            retry_pending(state);
        }
        InspectorActivation::Reconnect => {
            state.model.close_inspector();
            if let Some(session) = state.model.selected_session.clone() {
                state.load(session);
            }
        }
        InspectorActivation::Suspension => {
            state.model.close_inspector();
            state.dispatch(AppAction::OverlayOpened(Overlay::Suspension));
        }
    }
}
