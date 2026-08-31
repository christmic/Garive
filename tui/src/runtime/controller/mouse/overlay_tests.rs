use crossterm::event::KeyModifiers;

use super::*;
use crate::application::{Overlay, TerminalSize};

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
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
    assert_eq!(activated, (0..13).collect());

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
