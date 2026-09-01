use crossterm::event::KeyModifiers;
use garive_host_client::SuspensionView;

use super::*;
use crate::application::{
    ActionOverlayIntent, ConnectionState, ConversationLandmark, InspectorVariant, Overlay,
    TerminalSize,
};

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn overlay_point(model: &AppModel) -> (u16, u16) {
    (0..model.terminal_size.height)
        .flat_map(|row| (0..model.terminal_size.width).map(move |column| (column, row)))
        .find(|(column, row)| crate::view::overlay_contains(model, *column, *row))
        .expect("overlay geometry must expose one mouse-owned cell")
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
    let (column, row) = overlay_point(&model);
    assert_eq!(
        route(&model, mouse(MouseEventKind::ScrollUp, column, row)),
        Some(MouseAction::OverlayMove { backwards: true })
    );
    let activated = (0..24)
        .flat_map(|row| (0..100).map(move |column| (column, row)))
        .filter_map(|(column, row)| {
            match route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            ) {
                Some(MouseAction::OverlayActivate(index)) => Some(index),
                _ => None,
            }
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(activated, (4..12).collect());

    let compact = AppModel {
        terminal_size: TerminalSize {
            width: 40,
            height: 8,
        },
        command_selection: crate::input::COMMAND_PALETTE.len() - 1,
        ..model
    };
    let activated = (0..8)
        .flat_map(|row| (0..40).map(move |column| (column, row)))
        .filter_map(|(column, row)| {
            match route(
                &compact,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            ) {
                Some(MouseAction::OverlayActivate(index)) => Some(index),
                _ => None,
            }
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(activated, [19, 20].into_iter().collect());
}

#[test]
fn compact_history_mouse_never_maps_action_or_grapheme_continuation_rows() {
    let model = AppModel {
        overlay: Some(Overlay::PromptHistory),
        terminal_size: TerminalSize {
            width: 40,
            height: 8,
        },
        prompt_history: vec![
            "first".into(),
            format!("{} CJK提示", "👨‍👩‍👧‍👦界".repeat(20)),
            "third".into(),
        ],
        history_selection: 1,
        ..Default::default()
    };
    let rows_for_selected = (0..8)
        .filter(|row| {
            (0..40).any(|column| {
                route(
                    &model,
                    mouse(MouseEventKind::Down(MouseButton::Left), column, *row),
                ) == Some(MouseAction::OverlayActivate(1))
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(rows_for_selected.len(), 1);
    let activated = (0..8)
        .flat_map(|row| (0..40).map(move |column| (column, row)))
        .filter_map(|(column, row)| {
            match route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            ) {
                Some(MouseAction::OverlayActivate(index)) => Some(index),
                _ => None,
            }
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(activated, [1].into_iter().collect());
}

#[test]
fn compact_turn_and_inspector_mouse_use_only_their_rendered_windows() {
    let turns = AppModel {
        overlay: Some(Overlay::TurnNavigator),
        terminal_size: TerminalSize {
            width: 40,
            height: 8,
        },
        turn_selection: 19,
        conversation_landmarks: (0..20)
            .map(|index| ConversationLandmark {
                ordinal: index + 1,
                started_position: index as u64 + 1,
                prompt_preview: format!("turn {index}"),
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(activated_indices(&turns), [19].into_iter().collect());
    assert_eq!(activated_rows(&turns, 19).len(), 1);

    let mut inspector = AppModel {
        overlay: Some(Overlay::Inspector),
        terminal_size: TerminalSize {
            width: 40,
            height: 8,
        },
        connection: ConnectionState::Reconnecting { attempt: 2 },
        ..Default::default()
    };
    inspector.pending_recovery.current_session = true;
    inspector.pending_recovery.other_session = true;
    inspector.inspector.open = true;
    inspector.select_inspector_variant(InspectorVariant::Recovery);
    inspector.select_inspector_index(2);
    assert_eq!(activated_indices(&inspector), [1, 2].into_iter().collect());
    assert_eq!(activated_rows(&inspector, 1).len(), 2);
    assert_eq!(activated_rows(&inspector, 2).len(), 2);
}

#[test]
fn compact_decision_mouse_maps_visible_choice_and_both_safe_actions() {
    let mut model = AppModel {
        overlay: Some(Overlay::Suspension),
        terminal_size: TerminalSize {
            width: 40,
            height: 8,
        },
        selected_session: Some("session".into()),
        selected_turn: Some("turn".into()),
        suspension: Some(SuspensionView {
            suspension_id: "s".into(),
            session_version: 1,
            kind: "approval_required".into(),
            prompt_schema: "garive.public-suspension-prompt.v1".into(),
            prompt_json: r#"{"schema_version":1,"title_key":"title","message_text":"bounded","action_label_key":"allow"}"#.into(),
            prompt_digest: "0".repeat(64),
            response_schema_json: Some(r#"{"type":"boolean"}"#.into()),
            response_schema_digest: Some("1".repeat(64)),
        }),
        ..Default::default()
    };
    model.reconcile_suspension_response();
    model.suspension_response.as_mut().unwrap().choice_selection = 1;

    let routes = (0..8)
        .flat_map(|row| (0..40).map(move |column| (column, row)))
        .filter_map(|(column, row)| {
            route(
                &model,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            )
        })
        .collect::<Vec<_>>();
    for expected in [
        MouseAction::DecisionChoice(1),
        MouseAction::DecisionAction(ActionOverlayIntent::SubmitSuspension),
        MouseAction::DecisionAction(ActionOverlayIntent::LeaveSafely),
    ] {
        assert!(routes.contains(&expected));
    }
    assert!(routes.iter().all(|action| matches!(
        action,
        MouseAction::DecisionChoice(1)
            | MouseAction::DecisionAction(ActionOverlayIntent::SubmitSuspension)
            | MouseAction::DecisionAction(ActionOverlayIntent::LeaveSafely)
    )));
}

fn activated_indices(model: &AppModel) -> std::collections::BTreeSet<usize> {
    (0..8)
        .flat_map(|row| (0..40).map(move |column| (column, row)))
        .filter_map(|(column, row)| {
            match route(
                model,
                mouse(MouseEventKind::Down(MouseButton::Left), column, row),
            ) {
                Some(MouseAction::OverlayActivate(index)) => Some(index),
                _ => None,
            }
        })
        .collect()
}

fn activated_rows(model: &AppModel, target: usize) -> std::collections::BTreeSet<u16> {
    (0..8)
        .filter(|row| {
            (0..40).any(|column| {
                route(
                    model,
                    mouse(MouseEventKind::Down(MouseButton::Left), column, *row),
                ) == Some(MouseAction::OverlayActivate(target))
            })
        })
        .collect()
}
