use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::{
    application::{AppAction, AppModel, FocusTarget, Overlay},
    view::{navigation_hit_test, overlay_contains, overlay_hit_test},
};

use super::{
    navigation::{select_command, select_history, select_session},
    RuntimeState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseAction {
    ConversationScroll { backwards: bool },
    SessionActivate(usize),
    OverlayMove { backwards: bool },
    OverlayActivate(usize),
}

pub(super) fn handle(mouse: MouseEvent, state: &mut RuntimeState) {
    let Some(action) = route(&state.model, mouse) else {
        return;
    };
    match action {
        MouseAction::ConversationScroll { backwards: true } => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            state.model.scroll_conversation_up(3);
        }
        MouseAction::ConversationScroll { backwards: false } => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Conversation));
            state.model.scroll_conversation_down(3);
        }
        MouseAction::SessionActivate(index) => {
            state.dispatch(AppAction::FocusChanged(FocusTarget::Navigation));
            if let Some(session) = state.model.sessions.get(index) {
                state.model.session_selection = index;
                state.model.navigation_selection = Some(session.session_id.clone());
                state.load(session.session_id.clone());
            }
        }
        MouseAction::OverlayMove { backwards } => move_overlay_selection(state, backwards),
        MouseAction::OverlayActivate(index) => activate_overlay_selection(state, index),
    }
}

fn route(model: &AppModel, mouse: MouseEvent) -> Option<MouseAction> {
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
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(MouseAction::ConversationScroll { backwards: true }),
        MouseEventKind::ScrollDown => Some(MouseAction::ConversationScroll { backwards: false }),
        MouseEventKind::Down(MouseButton::Left) => {
            navigation_hit_test(model, mouse.column, mouse.row).map(MouseAction::SessionActivate)
        }
        _ => None,
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
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::application::TerminalSize;

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
                height: 12,
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
                mouse(MouseEventKind::Down(MouseButton::Left), 50, 6)
            ),
            Some(MouseAction::OverlayActivate(11))
        );
    }
}
