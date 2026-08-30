use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Padding},
};

use crate::{
    application::{AppModel, Overlay},
    input::COMMAND_PALETTE,
};

use super::super::primitives::{centered_popup, selection_window};

pub(super) struct OverlayGeometry {
    pub(super) popup: Rect,
    pub(super) inner: Rect,
    pub(super) window: Option<(usize, usize)>,
}

pub(super) fn overlay_geometry(model: &AppModel, overlay: Overlay, area: Rect) -> OverlayGeometry {
    let (desired_width, desired_height) = desired_size(model, overlay);
    let popup = centered_popup(
        area,
        desired_width.min(area.width.saturating_sub(4)),
        desired_height.min(area.height.saturating_sub(2)),
    );
    let inner = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::new(2, 2, 1, 1))
        .inner(popup);
    let window = list_count_and_selection(model, overlay).map(|(count, selected)| {
        selection_window(count, selected, usize::from(popup.height.saturating_sub(7)))
    });
    OverlayGeometry {
        popup,
        inner,
        window,
    }
}

pub(super) fn desired_size(model: &AppModel, overlay: Overlay) -> (u16, u16) {
    match overlay {
        Overlay::CommandPalette => (
            74,
            u16::try_from(COMMAND_PALETTE.len())
                .unwrap_or(u16::MAX)
                .saturating_add(7)
                .clamp(12, 22),
        ),
        Overlay::Help => (62, 10),
        Overlay::SessionPicker => (
            62,
            u16::try_from(model.matching_sessions().count())
                .unwrap_or(u16::MAX)
                .saturating_add(7)
                .clamp(8, 16),
        ),
        Overlay::PromptHistory => (
            62,
            u16::try_from(model.matching_history().count())
                .unwrap_or(u16::MAX)
                .saturating_add(7)
                .clamp(8, 16),
        ),
        Overlay::Suspension => (62, 12),
        Overlay::UnknownCommand
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => (62, 7),
    }
}

pub(in crate::view) fn selection_at(
    model: &AppModel,
    overlay: Overlay,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let geometry = overlay_geometry(model, overlay, area);
    let (start, end) = geometry.window?;
    let first_row = geometry.inner.y.saturating_add(1);
    if column < geometry.inner.x
        || column >= geometry.inner.right()
        || row < first_row
        || row >= first_row.saturating_add(u16::try_from(end - start).ok()?)
    {
        return None;
    }
    Some(start + usize::from(row - first_row))
}

pub(in crate::view) fn contains(
    model: &AppModel,
    overlay: Overlay,
    area: Rect,
    column: u16,
    row: u16,
) -> bool {
    overlay_geometry(model, overlay, area)
        .popup
        .contains((column, row).into())
}

fn list_count_and_selection(model: &AppModel, overlay: Overlay) -> Option<(usize, usize)> {
    match overlay {
        Overlay::CommandPalette => Some((
            model.matching_command_indices().len(),
            model.command_selection,
        )),
        Overlay::SessionPicker => {
            Some((model.matching_sessions().count(), model.session_selection))
        }
        Overlay::PromptHistory => Some((model.matching_history().count(), model.history_selection)),
        _ => None,
    }
}
