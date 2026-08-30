use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Padding},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    application::{AppModel, Overlay},
    input::COMMAND_PALETTE,
};

use super::super::{
    presentation::action_overlay_copy,
    primitives::{centered_popup, selection_window},
    safe_text,
};

pub(super) struct OverlayGeometry {
    pub(super) popup: Rect,
    pub(super) inner: Rect,
    pub(super) window: Option<(usize, usize)>,
}

pub(super) fn overlay_geometry(model: &AppModel, overlay: Overlay, area: Rect) -> OverlayGeometry {
    let desired_width = desired_width(overlay);
    let popup_width = desired_width.min(area.width.saturating_sub(4));
    let desired_height = desired_height(model, overlay, popup_width);
    let popup = centered_popup(
        area,
        popup_width,
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

fn desired_width(overlay: Overlay) -> u16 {
    match overlay {
        Overlay::CommandPalette => 74,
        Overlay::Help => 72,
        Overlay::SessionPicker
        | Overlay::PromptHistory
        | Overlay::Suspension
        | Overlay::UnknownCommand
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => 62,
    }
}

fn desired_height(model: &AppModel, overlay: Overlay, popup_width: u16) -> u16 {
    match overlay {
        Overlay::CommandPalette => u16::try_from(COMMAND_PALETTE.len())
            .unwrap_or(u16::MAX)
            .saturating_add(7)
            .clamp(12, 22),
        Overlay::Help => 13,
        Overlay::SessionPicker => u16::try_from(model.matching_sessions().count())
            .unwrap_or(u16::MAX)
            .saturating_add(7)
            .clamp(8, 16),
        Overlay::PromptHistory => u16::try_from(model.matching_history().count())
            .unwrap_or(u16::MAX)
            .saturating_add(7)
            .clamp(8, 16),
        Overlay::Suspension => 12,
        Overlay::UnknownCommand
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => {
            let copy = action_overlay_copy(model, overlay)
                .expect("action overlay variants always have shared presentation");
            let content_width = popup_width.saturating_sub(6).max(1);
            let body_rows = wrapped_rows(&safe_text(&copy.body), content_width);
            u16::try_from(body_rows)
                .unwrap_or(u16::MAX)
                .saturating_add(6)
        }
    }
}

fn wrapped_rows(value: &str, width: u16) -> usize {
    let width = usize::from(width.max(1));
    value
        .split('\n')
        .map(|line| {
            let width_only = UnicodeWidthStr::width(line).max(1).div_ceil(width);
            let mut word_rows = 1usize;
            let mut used = 0usize;
            for word in line.split_whitespace() {
                let word_width = UnicodeWidthStr::width(word);
                if used > 0 {
                    if used.saturating_add(1).saturating_add(word_width) <= width {
                        used += 1 + word_width;
                        continue;
                    }
                    word_rows += 1;
                }
                word_rows += word_width.saturating_sub(1) / width;
                used = word_width.saturating_sub(1) % width + usize::from(word_width > 0);
            }
            width_only.max(word_rows)
        })
        .sum::<usize>()
        .max(1)
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
