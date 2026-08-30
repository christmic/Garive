use crate::{
    application::{AppAction, AppModel, ExecutionState, FocusTarget, Overlay},
    input::{command_matches, parse_command, CommandParse, COMMAND_PALETTE},
};

use super::{actions::execute_command, RuntimeState};

pub(super) fn cycle_focus(state: &mut RuntimeState, backwards: bool) {
    let next = next_focus(
        state.model.terminal_size.width,
        state.model.focus,
        backwards,
    );
    state.dispatch(AppAction::FocusChanged(next));
    if next == FocusTarget::Navigation {
        ensure_navigation_selection(&mut state.model);
    }
}

pub(super) fn move_navigation_selection(model: &mut AppModel, backwards: bool) {
    ensure_navigation_selection(model);
    let Some(current) = model.navigation_selection.as_deref() else {
        return;
    };
    let Some(index) = model
        .sessions
        .iter()
        .position(|session| session.session_id == current)
    else {
        return;
    };
    let target = if backwards {
        index.saturating_sub(1)
    } else {
        (index + 1).min(model.sessions.len().saturating_sub(1))
    };
    model.navigation_selection = model
        .sessions
        .get(target)
        .map(|session| session.session_id.clone());
}

pub(super) fn move_navigation_to_edge(model: &mut AppModel, last: bool) {
    let session = if last {
        model.sessions.last()
    } else {
        model.sessions.first()
    };
    model.navigation_selection = session.map(|value| value.session_id.clone());
}

pub(super) fn activate_navigation_selection(state: &mut RuntimeState) {
    ensure_navigation_selection(&mut state.model);
    if let Some(session_id) = state.model.navigation_selection.clone() {
        state.load(session_id);
    }
}

fn ensure_navigation_selection(model: &mut AppModel) {
    let current_is_visible = model.navigation_selection.as_deref().is_some_and(|id| {
        model
            .sessions
            .iter()
            .any(|session| session.session_id == id)
    });
    if current_is_visible {
        return;
    }
    model.navigation_selection = model
        .selected_session
        .as_ref()
        .filter(|id| {
            model
                .sessions
                .iter()
                .any(|session| &session.session_id == *id)
        })
        .cloned()
        .or_else(|| {
            model
                .sessions
                .first()
                .map(|session| session.session_id.clone())
        });
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

pub(super) fn conversation_page_cells(state: &RuntimeState) -> usize {
    usize::from(state.model.terminal_size.height.saturating_sub(8) / 3).max(1)
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
    COMMAND_PALETTE
        .iter()
        .enumerate()
        .filter(|(_, (name, help))| command_matches(name, help, &state.model.command_filter))
        .map(|(index, _)| index)
        .collect()
}

pub(super) fn matching_history(state: &RuntimeState) -> Vec<String> {
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
    use garive_host_client::SessionSummary;

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

    #[test]
    fn navigation_selection_is_stable_bounded_and_seeded_from_active_session() {
        let mut model = AppModel {
            sessions: vec![
                session("session-a"),
                session("session-b"),
                session("session-c"),
            ],
            selected_session: Some("session-b".into()),
            ..Default::default()
        };
        move_navigation_selection(&mut model, true);
        assert_eq!(model.navigation_selection.as_deref(), Some("session-a"));
        move_navigation_selection(&mut model, true);
        assert_eq!(model.navigation_selection.as_deref(), Some("session-a"));
        move_navigation_to_edge(&mut model, true);
        assert_eq!(model.navigation_selection.as_deref(), Some("session-c"));

        model.navigation_selection = Some("stale".into());
        move_navigation_selection(&mut model, false);
        assert_eq!(model.navigation_selection.as_deref(), Some("session-c"));
    }

    fn session(id: &str) -> SessionSummary {
        SessionSummary {
            api_version: "v1".into(),
            session_id: id.into(),
            agent_instance_id: "agent-instance".into(),
            definition_id: "definition".into(),
            definition_revision: "revision".into(),
            opened_at: "2026-08-30T00:00:00Z".into(),
            latest_position: 1,
            latest_turn_id: Some("turn".into()),
            latest_turn_state: Some("running".into()),
            turn_count: 1,
        }
    }
}
