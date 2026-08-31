use crate::application::Overlay;

use super::super::{
    inspector,
    navigation::{select_command, select_history, select_landmark, select_session},
    overlay as controller_overlay, RuntimeState,
};

pub(super) fn move_selection(state: &mut RuntimeState, backwards: bool) {
    let Some(overlay) = state.model.overlay else {
        return;
    };
    match overlay {
        Overlay::CommandPalette => {
            let count = state.model.matching_command_indices().len();
            state.model.command_selection = moved(state.model.command_selection, count, backwards);
        }
        Overlay::PromptHistory => {
            let count = state.model.matching_history().count();
            state.model.history_selection = moved(state.model.history_selection, count, backwards);
        }
        Overlay::SessionPicker => {
            let count = state.model.matching_sessions().count();
            if !backwards && state.model.session_selection >= count.saturating_sub(1) {
                state.load_more_sessions();
                return;
            }
            state.model.session_selection = moved(state.model.session_selection, count, backwards);
        }
        Overlay::TurnNavigator => {
            let count = state.model.matching_landmark_indices().len();
            state.model.turn_selection = moved(state.model.turn_selection, count, backwards);
        }
        Overlay::Inspector => inspector::move_selection(state, backwards),
        Overlay::Suspension => controller_overlay::move_suspension_choice(state, backwards),
        _ => {}
    }
}

pub(super) fn activate_selection(state: &mut RuntimeState, index: usize) {
    match state.model.overlay {
        Some(Overlay::CommandPalette) => {
            state.model.command_selection = index;
            select_command(state);
        }
        Some(Overlay::PromptHistory) => {
            state.model.history_selection = index;
            select_history(state);
        }
        Some(Overlay::SessionPicker) => {
            state.model.session_selection = index;
            select_session(state);
        }
        Some(Overlay::TurnNavigator) => {
            state.model.turn_selection = index;
            select_landmark(state);
        }
        Some(Overlay::Inspector) => {
            inspector::select_index(state, index);
            inspector::activate(state);
        }
        _ => {}
    }
}

fn moved(current: usize, count: usize, backwards: bool) -> usize {
    if backwards {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(count.saturating_sub(1))
    }
}
