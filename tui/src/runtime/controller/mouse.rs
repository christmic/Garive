use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::{
    application::{ActionOverlayIntent, AppAction, AppModel, FocusTarget},
    input::ComposerClick,
    view::{
        command_suggestion_hit_test, composer_hit_test, conversation_follow_cue_hit_test,
        decision_action_hit_test, decision_choice_hit_test, inspector_contains, inspector_hit_test,
        overlay_contains, overlay_hit_test,
    },
};

use super::{accept_command_suggestion, scroll_conversation, RuntimeState};

mod overlay;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseAction {
    ConversationScroll { backwards: bool },
    FollowLatest,
    OverlayMove { backwards: bool },
    OverlayActivate(usize),
    DecisionAction(ActionOverlayIntent),
    DecisionChoice(usize),
    InspectorMove { backwards: bool },
    InspectorActivate(usize),
    Consume,
    SuggestionMove { backwards: bool },
    SuggestionActivate(usize),
    ComposerPlace(usize),
}

pub(super) fn handle(mouse: MouseEvent, state: &mut RuntimeState) {
    if state.model.overlay.is_some() || state.composer_is_frozen() {
        state.composer_mouse_selecting = false;
    }
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
        MouseAction::FollowLatest => state.model.follow_latest(),
        MouseAction::OverlayMove { backwards } => overlay::move_selection(state, backwards),
        MouseAction::OverlayActivate(index) => overlay::activate_selection(state, index),
        MouseAction::DecisionAction(intent) => {
            if let Some(overlay) = state.model.overlay {
                super::overlay::activate_intent(intent, overlay, state);
            }
        }
        MouseAction::DecisionChoice(index) => {
            if let Some(response) = state.model.suspension_response.as_mut() {
                response.choice_selection = index;
            }
        }
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
                decision_action_hit_test(model, mouse.column, mouse.row)
                    .map(MouseAction::DecisionAction)
                    .or_else(|| {
                        decision_choice_hit_test(model, mouse.column, mouse.row)
                            .map(MouseAction::DecisionChoice)
                    })
                    .or_else(|| {
                        overlay_hit_test(model, mouse.column, mouse.row)
                            .map(MouseAction::OverlayActivate)
                    })
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
            if conversation_follow_cue_hit_test(model, mouse.column, mouse.row) {
                Some(MouseAction::FollowLatest)
            } else {
                composer_hit_test(model, mouse.column, mouse.row, false)
                    .map(MouseAction::ComposerPlace)
            }
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

#[cfg(test)]
#[path = "mouse/overlay_tests.rs"]
mod overlay_tests;

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use garive_host_client::SuspensionView;

    use super::*;
    use crate::application::{
        ConversationLandmark, Overlay, TerminalSize, TimelineItem, TimelineRole,
    };

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
    fn follow_cue_click_restores_latest_without_mutating_the_composer() {
        let mut state = RuntimeState::test_ephemeral(Vec::new());
        state.model.terminal_size = TerminalSize {
            width: 100,
            height: 24,
        };
        state.model.focus = FocusTarget::Composer;
        state.model.viewport.follow_latest = false;
        state.model.viewport.newer_updates = 3;
        state.model.composer.replace("retained draft").unwrap();
        state.model.composer.move_document_start(false);
        state.model.composer.move_right(true);
        let cursor = state.model.composer.cursor_grapheme();
        let selection = state.model.composer.selected_byte_range();
        let (column, row) = (0..24)
            .find_map(|row| {
                (0..100)
                    .find(|column| conversation_follow_cue_hit_test(&state.model, *column, row))
                    .map(|column| (column, row))
            })
            .expect("detached FollowCue has a hit target");

        handle(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut state,
        );

        assert!(state.model.viewport.follow_latest);
        assert_eq!(state.model.viewport.newer_updates, 0);
        assert_eq!(state.model.focus, FocusTarget::Composer);
        assert_eq!(state.model.composer.text(), "retained draft");
        assert_eq!(state.model.composer.cursor_grapheme(), cursor);
        assert_eq!(state.model.composer.selected_byte_range(), selection);

        state.model.viewport.follow_latest = false;
        state.model.viewport.newer_updates = 4;
        state.model.overlay = Some(Overlay::Help);
        assert!(!conversation_follow_cue_hit_test(&state.model, column, row));
        handle(
            mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            &mut state,
        );
        assert!(!state.model.viewport.follow_latest);
        assert_eq!(state.model.viewport.newer_updates, 4);
    }

    #[test]
    fn modal_opened_during_composer_drag_cancels_background_selection() {
        let mut state = RuntimeState::test_ephemeral(Vec::new());
        state.model.terminal_size = TerminalSize {
            width: 100,
            height: 24,
        };
        state.model.composer.replace("a界b").unwrap();
        state.model.composer.move_document_start(false);
        state.composer_mouse_selecting = true;
        state.model.overlay = Some(Overlay::Help);

        handle(
            mouse(MouseEventKind::Drag(MouseButton::Left), 5, 21),
            &mut state,
        );

        assert!(!state.composer_mouse_selecting);
        assert_eq!(state.model.composer.cursor_grapheme(), 0);
        assert!(!state.model.composer.has_selection());
    }

    #[test]
    fn decision_sheet_mouse_routes_only_visible_choice_and_action_rows() {
        let mut model = AppModel {
            overlay: Some(Overlay::Suspension),
            terminal_size: TerminalSize {
                width: 52,
                height: 8,
            },
            selected_session: Some("session".into()),
            selected_turn: Some("turn".into()),
            suspension: Some(SuspensionView {
                suspension_id: "s".into(),
                session_version: 1,
                kind: "approval_required".into(),
                prompt_schema: "garive.public-suspension-prompt.v1".into(),
                prompt_json: format!(
                    r#"{{"schema_version":1,"title_key":"title","message_text":"{}","action_label_key":"allow"}}"#,
                    "需要确认的公开说明".repeat(12)
                ),
                prompt_digest: "0".repeat(64),
                response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
                response_schema_digest: Some("1".repeat(64)),
            }),
            ..Default::default()
        };
        model.reconcile_suspension_response();
        model.suspension_response.as_mut().unwrap().choice_selection = 1;
        let mut routes = Vec::new();
        for row in 0..8 {
            for column in 0..52 {
                if let Some(action) = route(
                    &model,
                    mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                ) {
                    routes.push(action);
                }
            }
        }
        assert!(routes.contains(&MouseAction::DecisionChoice(1)));
        assert!(routes.contains(&MouseAction::DecisionAction(
            ActionOverlayIntent::SubmitSuspension
        )));
        assert!(routes.contains(&MouseAction::DecisionAction(
            ActionOverlayIntent::LeaveSafely
        )));
        assert!(!routes.contains(&MouseAction::ComposerPlace(0)));
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
                mouse(MouseEventKind::Down(MouseButton::Left), 5, 22)
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
