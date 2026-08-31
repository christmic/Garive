use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::{
    application::{AppAction, AppModel, FocusTarget, Overlay},
    input::ComposerClick,
    view::{
        command_suggestion_hit_test, composer_hit_test, inspector_contains, inspector_hit_test,
        overlay_contains, overlay_hit_test,
    },
};

use super::{
    accept_command_suggestion,
    navigation::{select_command, select_history, select_landmark, select_session},
    scroll_conversation, RuntimeState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseAction {
    ConversationScroll { backwards: bool },
    OverlayMove { backwards: bool },
    OverlayActivate(usize),
    InspectorMove { backwards: bool },
    InspectorActivate(usize),
    Consume,
    SuggestionMove { backwards: bool },
    SuggestionActivate(usize),
    ComposerPlace(usize),
}

pub(super) fn handle(mouse: MouseEvent, state: &mut RuntimeState) {
    if state.composer_mouse_selecting {
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                if let Some(grapheme) =
                    composer_hit_test(&state.model, mouse.column, mouse.row, true)
                {
                    state.model.composer.place_cursor(grapheme, true);
                }
                if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
                    state.composer_mouse_selecting = false;
                }
                return;
            }
            MouseEventKind::Down(_) => state.composer_mouse_selecting = false,
            _ => {}
        }
    }
    let action = route(&state.model, mouse);
    let Some(action) = action else {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            state.composer_clicks.reset();
        }
        return;
    };
    if !matches!(action, MouseAction::ComposerPlace(_)) {
        state.composer_clicks.reset();
    }
    match action {
        MouseAction::ConversationScroll { backwards: true } => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            scroll_conversation(state, -3);
        }
        MouseAction::ConversationScroll { backwards: false } => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            scroll_conversation(state, 3);
        }
        MouseAction::OverlayMove { backwards } => move_overlay_selection(state, backwards),
        MouseAction::OverlayActivate(index) => activate_overlay_selection(state, index),
        MouseAction::InspectorMove { backwards } => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Inspector));
            super::inspector::move_selection(state, backwards);
        }
        MouseAction::InspectorActivate(index) => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Inspector));
            super::inspector::select_index(state, index);
            super::inspector::activate(state);
        }
        MouseAction::Consume => {}
        MouseAction::SuggestionMove { backwards } => {
            let count = state.model.matching_command_suggestion_indices().len();
            state.model.command_suggestion_selection =
                moved_selection_wrapped(state.model.command_suggestion_selection, count, backwards);
        }
        MouseAction::SuggestionActivate(index) => {
            state.model.command_suggestion_selection = index;
            accept_command_suggestion(state);
        }
        MouseAction::ComposerPlace(grapheme) => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Composer));
            match state.composer_clicks.register(mouse.column, mouse.row) {
                ComposerClick::Place => {
                    state.model.composer.place_cursor(grapheme, false);
                    state.composer_mouse_selecting = true;
                }
                ComposerClick::SelectWord => {
                    state.model.composer.select_word_at(grapheme);
                    state.composer_mouse_selecting = false;
                }
                ComposerClick::SelectLine => {
                    state.model.composer.select_logical_line_at(grapheme);
                    state.composer_mouse_selecting = false;
                }
            }
        }
    }
}

fn route(model: &AppModel, mouse: MouseEvent) -> Option<MouseAction> {
    if matches!(mouse.kind, MouseEventKind::Moved) {
        return None;
    }
    if model.overlay.is_some() {
        if !overlay_contains(model, mouse.column, mouse.row) {
            return None;
        }
        return match mouse.kind {
            MouseEventKind::ScrollUp => Some(MouseAction::OverlayMove { backwards: true }),
            MouseEventKind::ScrollDown => Some(MouseAction::OverlayMove { backwards: false }),
            MouseEventKind::Down(MouseButton::Left) => {
                overlay_hit_test(model, mouse.column, mouse.row).map(MouseAction::OverlayActivate)
            }
            _ => None,
        };
    }
    if model.inspector.open && inspector_contains(model, mouse.column, mouse.row) {
        return match mouse.kind {
            MouseEventKind::ScrollUp => Some(MouseAction::InspectorMove { backwards: true }),
            MouseEventKind::ScrollDown => Some(MouseAction::InspectorMove { backwards: false }),
            MouseEventKind::Down(MouseButton::Left) => {
                inspector_hit_test(model, mouse.column, mouse.row)
                    .map_or(Some(MouseAction::Consume), |index| {
                        Some(MouseAction::InspectorActivate(index))
                    })
            }
            _ => Some(MouseAction::Consume),
        };
    }
    let suggestion = command_suggestion_hit_test(model, mouse.column, mouse.row);
    if suggestion.is_some() {
        return match mouse.kind {
            MouseEventKind::ScrollUp => Some(MouseAction::SuggestionMove { backwards: true }),
            MouseEventKind::ScrollDown => Some(MouseAction::SuggestionMove { backwards: false }),
            MouseEventKind::Down(MouseButton::Left) => {
                suggestion.map(MouseAction::SuggestionActivate)
            }
            _ => None,
        };
    }
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(MouseAction::ConversationScroll { backwards: true }),
        MouseEventKind::ScrollDown => Some(MouseAction::ConversationScroll { backwards: false }),
        MouseEventKind::Down(MouseButton::Left) => {
            composer_hit_test(model, mouse.column, mouse.row, false).map(MouseAction::ComposerPlace)
        }
        _ => None,
    }
}

fn moved_selection_wrapped(current: usize, count: usize, backwards: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if backwards {
        current.checked_sub(1).unwrap_or(count - 1)
    } else {
        (current + 1) % count
    }
}

fn move_overlay_selection(state: &mut RuntimeState, backwards: bool) {
    let Some(overlay) = state.model.overlay else {
        return;
    };
    match overlay {
        Overlay::CommandPalette => {
            let count = state.model.matching_command_indices().len();
            state.model.command_selection =
                moved_selection(state.model.command_selection, count, backwards);
        }
        Overlay::PromptHistory => {
            let count = state.model.matching_history().count();
            state.model.history_selection =
                moved_selection(state.model.history_selection, count, backwards);
        }
        Overlay::SessionPicker => {
            let count = state.model.matching_sessions().count();
            if !backwards && state.model.session_selection >= count.saturating_sub(1) {
                state.load_more_sessions();
                return;
            }
            state.model.session_selection =
                moved_selection(state.model.session_selection, count, backwards);
        }
        Overlay::TurnNavigator => {
            let count = state.model.matching_landmark_indices().len();
            state.model.turn_selection =
                moved_selection(state.model.turn_selection, count, backwards);
        }
        Overlay::Inspector => super::inspector::move_selection(state, backwards),
        _ => {}
    }
}

fn moved_selection(current: usize, count: usize, backwards: bool) -> usize {
    if backwards {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(count.saturating_sub(1))
    }
}

fn activate_overlay_selection(state: &mut RuntimeState, index: usize) {
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
            super::inspector::select_index(state, index);
            super::inspector::activate(state);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::application::{ConversationLandmark, TerminalSize, TimelineItem, TimelineRole};

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn modal_mouse_events_never_route_to_the_background() {
        let model = AppModel {
            overlay: Some(Overlay::Help),
            terminal_size: TerminalSize {
                width: 100,
                height: 24,
            },
            ..Default::default()
        };
        assert_eq!(route(&model, mouse(MouseEventKind::ScrollDown, 1, 1)), None);
        assert_eq!(
            route(&model, mouse(MouseEventKind::Down(MouseButton::Left), 5, 5)),
            None
        );
    }

    #[test]
    fn selectable_overlay_routes_visible_rows_and_wheel() {
        let model = AppModel {
            overlay: Some(Overlay::CommandPalette),
            terminal_size: TerminalSize {
                width: 100,
                height: 24,
            },
            command_selection: 11,
            ..Default::default()
        };
        assert_eq!(
            route(&model, mouse(MouseEventKind::ScrollUp, 50, 6)),
            Some(MouseAction::OverlayMove { backwards: true })
        );
        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), 50, 14)
            ),
            Some(MouseAction::OverlayActivate(11))
        );
    }

    #[test]
    fn turn_navigator_mouse_uses_its_filtered_visible_window() {
        let model = AppModel {
            overlay: Some(Overlay::TurnNavigator),
            terminal_size: TerminalSize {
                width: 100,
                height: 24,
            },
            turn_selection: 19,
            conversation_landmarks: (0..20)
                .map(|index| ConversationLandmark {
                    ordinal: index + 1,
                    started_position: index as u64 + 1,
                    prompt_preview: format!("prompt {index:02}"),
                })
                .collect(),
            ..Default::default()
        };
        assert_eq!(
            route(&model, mouse(MouseEventKind::ScrollUp, 50, 10)),
            Some(MouseAction::OverlayMove { backwards: true })
        );
        assert!((0..24).any(|row| (0..100).any(|column| {
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            ) == Some(MouseAction::OverlayActivate(19))
        })));
        assert!(!(0..24).any(|row| (0..100).any(|column| {
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            ) == Some(MouseAction::OverlayActivate(0))
        })));
    }

    #[test]
    fn composer_click_routes_through_component_geometry() {
        let mut model = AppModel {
            terminal_size: TerminalSize {
                width: 100,
                height: 24,
            },
            ..Default::default()
        };
        model.composer.replace("a界b").unwrap();

        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), 5, 21)
            ),
            Some(MouseAction::ComposerPlace(1))
        );
    }

    #[test]
    fn wide_inspector_entries_activate_but_border_and_padding_are_inert() {
        let mut model = AppModel {
            terminal_size: TerminalSize {
                width: 129,
                height: 24,
            },
            ..Default::default()
        };
        model.inspector.open = true;

        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), 99, 1)
            ),
            Some(MouseAction::InspectorActivate(0))
        );
        for (column, row) in [(97, 0), (98, 1)] {
            assert_eq!(
                route(
                    &model,
                    mouse(MouseEventKind::Down(MouseButton::Left), column, row)
                ),
                Some(MouseAction::Consume)
            );
        }
        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), 96, 1)
            ),
            None
        );
    }

    #[test]
    fn removed_rails_have_no_pointer_routes() {
        let mut model = AppModel {
            terminal_size: TerminalSize {
                width: 100,
                height: 24,
            },
            ..Default::default()
        };
        for position in 0..20 {
            model.push_test_timeline_item(TimelineItem {
                stable_key: format!("cell-{position}"),
                position: position + 1,
                role: TimelineRole::Agent,
                tone: Default::default(),
                text: "bounded".into(),
            });
        }
        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), 98, 3)
            ),
            None
        );
        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Drag(MouseButton::Left), 98, 10)
            ),
            None
        );
        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Drag(MouseButton::Left), 97, 10)
            ),
            None
        );
        assert_eq!(
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), 98, 18)
            ),
            None
        );
        assert_eq!(route(&model, mouse(MouseEventKind::Moved, 98, 11)), None);
        assert_eq!(route(&model, mouse(MouseEventKind::Moved, 97, 11)), None);
    }
}
