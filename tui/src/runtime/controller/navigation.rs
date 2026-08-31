use crate::{
    application::{AppAction, FocusTarget, Overlay},
    input::{parse_command, CommandParse, COMMAND_PALETTE},
};

use super::{actions::execute_command, RuntimeState};

pub(super) fn cycle_focus(state: &mut RuntimeState, backwards: bool) {
    let next = next_focus(
        state.model.terminal_size.width,
        state.model.focus,
        backwards,
        state.model.inspector.open,
    );
    state.dispatch(AppAction::FocusChanged(next));
}

fn next_focus(
    width: u16,
    current: FocusTarget,
    backwards: bool,
    inspector_open: bool,
) -> FocusTarget {
    if width >= 120 && inspector_open {
        return match (backwards, current) {
            (false, FocusTarget::Composer) => FocusTarget::Conversation,
            (false, FocusTarget::Conversation) => FocusTarget::Inspector,
            (false, _) => FocusTarget::Composer,
            (true, FocusTarget::Composer) => FocusTarget::Inspector,
            (true, FocusTarget::Inspector) => FocusTarget::Conversation,
            (true, _) => FocusTarget::Composer,
        };
    }
    match (backwards, current) {
        (false, FocusTarget::Conversation) => FocusTarget::Composer,
        (false, _) => FocusTarget::Conversation,
        (true, FocusTarget::Composer) => FocusTarget::Conversation,
        (true, _) => FocusTarget::Composer,
    }
}

pub(super) fn is_safe_query_character(character: char) -> bool {
    !character.is_control()
        && !matches!(
            character,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        )
}

pub(super) fn select_command(state: &mut RuntimeState) {
    let Some(index) = matching_commands(state)
        .get(state.model.command_selection)
        .copied()
    else {
        return;
    };
    let command = COMMAND_PALETTE[index];
    if let Some(reason) = command.unavailable_reason(state.model.command_context()) {
        state.model.notice = Some(format!("Command unavailable: {reason}."));
        return;
    }
    state.model.overlay = None;
    if let CommandParse::Valid(command) = parse_command(command.input) {
        execute_command(command, state);
    }
}

pub(super) fn select_history(state: &mut RuntimeState) {
    if state.composer_is_frozen() {
        state.explain_frozen_composer();
        state.model.overlay = None;
        return;
    }
    if let Some(text) = matching_history(state)
        .get(state.model.history_selection)
        .cloned()
    {
        let _ = state.model.composer.replace(&text);
        state.model.prompt_history_browser.reset();
    }
    state.model.overlay = None;
}

pub(super) fn open_command_palette(state: &mut RuntimeState) {
    state.model.command_filter.clear();
    state.model.command_selection = 0;
    state.dispatch(AppAction::OverlayOpened(Overlay::CommandPalette));
}

pub(super) fn open_prompt_history(state: &mut RuntimeState) {
    state.model.history_filter.clear();
    state.model.history_selection = 0;
    state.dispatch(AppAction::OverlayOpened(Overlay::PromptHistory));
}

pub(super) fn matching_commands(state: &RuntimeState) -> Vec<usize> {
    state.model.matching_command_indices()
}

pub(super) fn matching_history(state: &RuntimeState) -> Vec<String> {
    state.model.matching_history().cloned().collect()
}

pub(super) fn matching_landmarks(state: &RuntimeState) -> Vec<usize> {
    state.model.matching_landmark_indices()
}

pub(super) fn open_turn_navigator(state: &mut RuntimeState, filter: Option<String>) {
    if state.model.conversation_landmarks.len() < 2 {
        state.model.notice = Some("Turn navigation requires at least two loaded Turns.".into());
        return;
    }
    state.model.turn_filter = filter.unwrap_or_default();
    let matches = matching_landmarks(state);
    state.model.turn_selection = if state.model.viewport.follow_latest {
        matches.len().saturating_sub(1)
    } else {
        let anchor_position = state
            .model
            .viewport
            .anchor_key
            .as_deref()
            .and_then(|key| state.model.durable_child(key))
            .map(|item| item.position)
            .unwrap_or(0);
        matches
            .iter()
            .rposition(|index| {
                state.model.conversation_landmarks[*index].started_position <= anchor_position
            })
            .unwrap_or(0)
    };
    state.dispatch(AppAction::OverlayOpened(Overlay::TurnNavigator));
}

pub(super) fn select_landmark(state: &mut RuntimeState) {
    let Some(index) = matching_landmarks(state)
        .get(state.model.turn_selection)
        .copied()
    else {
        return;
    };
    let position = state.model.conversation_landmarks[index].started_position;
    if !state.model.jump_to_turn_position(position) {
        state.model.notice = Some("That Turn is no longer in the loaded timeline.".into());
    }
    state.model.close_turn_navigator();
}

pub(super) fn select_session(state: &mut RuntimeState) {
    let selected = matching_sessions(state)
        .get(state.model.session_selection)
        .cloned();
    state.model.overlay = None;
    if let Some(id) = selected {
        state.load(id);
    }
}

pub(super) fn open_session_picker(state: &mut RuntimeState) {
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

pub(super) fn cycle_session_selection(state: &mut RuntimeState, backwards: bool) {
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

pub(super) fn matching_sessions(state: &RuntimeState) -> Vec<String> {
    state
        .model
        .matching_sessions()
        .map(|session| session.session_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycle_adds_wide_inspector_without_changing_existing_order() {
        assert_eq!(
            next_focus(120, FocusTarget::Composer, false, true),
            FocusTarget::Conversation
        );
        assert_eq!(
            next_focus(120, FocusTarget::Conversation, false, true),
            FocusTarget::Inspector
        );
        assert_eq!(
            next_focus(120, FocusTarget::Inspector, false, true),
            FocusTarget::Composer
        );
        assert_eq!(
            next_focus(120, FocusTarget::Composer, true, true),
            FocusTarget::Inspector
        );
        assert_eq!(
            next_focus(120, FocusTarget::Inspector, true, true),
            FocusTarget::Conversation
        );
        assert_eq!(
            next_focus(120, FocusTarget::Conversation, true, true),
            FocusTarget::Composer
        );
        assert_eq!(
            next_focus(80, FocusTarget::Composer, false, true),
            FocusTarget::Conversation
        );
        assert_eq!(
            next_focus(120, FocusTarget::Conversation, false, false),
            FocusTarget::Composer
        );
        assert_eq!(
            next_focus(80, FocusTarget::Conversation, true, true),
            FocusTarget::Composer
        );
    }
}
