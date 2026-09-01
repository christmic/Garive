use super::*;
use crate::application::{Overlay, TerminalSize};
use ratatui::style::Modifier;

fn model(width: u16, height: u16, selection: usize) -> AppModel {
    AppModel {
        overlay: Some(Overlay::CommandPalette),
        terminal_size: TerminalSize { width, height },
        command_selection: selection,
        ..Default::default()
    }
}

#[test]
fn compact_layout_always_reserves_real_command_rows_and_safe_actions() {
    let model = model(40, 8, COMMAND_PALETTE.len() - 1);
    let layout = layout(&model, Rect::new(0, 0, 40, 8));
    assert!(layout.item_capacity() >= 1);
    assert_eq!(layout.window.1, COMMAND_PALETTE.len());
    assert!(layout.first_item_row < layout.action_row);
    assert!(layout.action_row.saturating_add(1) < layout.inner.bottom());
}

#[test]
fn every_command_can_own_a_visible_window() {
    for selected in 0..COMMAND_PALETTE.len() {
        let model = model(160, 28, selected);
        let layout = layout(&model, Rect::new(0, 0, 160, 28));
        assert!(layout.full_catalog);
        assert_eq!(layout.window, (0, COMMAND_PALETTE.len()));
        assert!(layout.window.0 <= selected);
        assert!(selected < layout.window.1);
    }
}

#[test]
fn full_catalog_uses_the_safe_area_without_covering_the_composer() {
    let model = model(160, 28, COMMAND_PALETTE.len() - 1);
    let area = Rect::new(0, 0, 160, 28);
    let frame = FrameLayout::resolve(&model, area);
    let layout = layout(&model, area);

    assert!(layout.full_catalog);
    assert_eq!(layout.item_capacity(), COMMAND_PALETTE.len());
    assert_eq!(layout.popup.y, frame.transcript.y);
    assert_eq!(layout.popup.bottom(), frame.composer.y);
    assert!(layout.popup.x >= frame.transcript.x);
    assert!(layout.popup.right() <= frame.transcript.right());
    assert_eq!(
        layout
            .first_item_row
            .saturating_add(COMMAND_PALETTE.len() as u16),
        layout.inner.bottom()
    );
}

#[test]
fn hit_testing_only_maps_rendered_item_rows() {
    let model = model(40, 8, COMMAND_PALETTE.len() - 1);
    let area = Rect::new(0, 0, 40, 8);
    let layout = layout(&model, area);
    assert_eq!(
        selection_at(&model, area, layout.inner.x, layout.first_item_row),
        Some(layout.window.0)
    );
    assert_eq!(
        selection_at(&model, area, layout.inner.x, layout.inner.y),
        None
    );
    assert_eq!(
        selection_at(&model, area, layout.inner.x, layout.action_row),
        None
    );
    assert_eq!(
        selection_at(&model, area, layout.inner.right(), layout.first_item_row),
        None
    );
}

#[test]
fn full_catalog_hit_testing_maps_first_and_last_commands() {
    let model = model(160, 28, COMMAND_PALETTE.len() - 1);
    let area = Rect::new(0, 0, 160, 28);
    let layout = layout(&model, area);

    assert_eq!(
        selection_at(&model, area, layout.inner.x, layout.first_item_row),
        Some(0)
    );
    assert_eq!(
        selection_at(
            &model,
            area,
            layout.inner.x,
            layout.inner.bottom().saturating_sub(1),
        ),
        Some(COMMAND_PALETTE.len() - 1)
    );
    assert_eq!(
        selection_at(&model, area, layout.inner.x, layout.inner.bottom()),
        None
    );
}

#[test]
fn linear_projection_announces_window_and_selected_absolute_index() {
    let model = model(40, 8, COMMAND_PALETTE.len() - 1);
    let spoken = linear_text(&model);
    assert!(spoken.contains("Showing commands"));
    assert!(spoken.contains("Selected 21 of 21: /quit"));
    assert!(spoken.contains("Home and End for edges"));
}

#[test]
fn selected_marker_and_unicode_matches_have_independent_emphasis() {
    let layout = PaletteLayout {
        popup: Rect::new(0, 0, 40, 8),
        inner: Rect::new(0, 0, 36, 6),
        window: (0, 1),
        first_item_row: 2,
        action_row: 4,
        compact: true,
        full_catalog: false,
    };
    let item = PaletteItem {
        input: "/状态😀",
        help: "打开项目",
        detail: "打开项目".into(),
        unavailable_reason: None,
    };
    let colors = super::super::super::palette(crate::Theme::Mono);
    let line = item_line(&item, "态😀 项目", true, layout, colors);

    assert!(line.spans[0]
        .style
        .add_modifier
        .contains(Modifier::REVERSED));
    let bold = line
        .spans
        .iter()
        .skip(1)
        .filter(|span| span.style.add_modifier.contains(Modifier::BOLD))
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert_eq!(bold, "态😀项目");
    assert_eq!(line.width(), usize::from(layout.inner.width));
}
