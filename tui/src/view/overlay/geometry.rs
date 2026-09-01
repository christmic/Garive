use crate::application::ActionOverlayIntent;
use ratatui::{layout::Rect, widgets::Padding};
use unicode_width::UnicodeWidthStr;

use crate::application::{AppModel, Overlay};

use super::super::{
    decision_sheet,
    layout::FrameLayout,
    primitives::{centered_popup, selection_window, BottomPaneFrame, ModalFrame},
};

use super::filtered_list::FilteredListGeometry;

pub(super) struct OverlayGeometry {
    pub(super) popup: Rect,
    pub(super) inner: Rect,
    pub(super) window: Option<(usize, usize)>,
}

pub(super) fn overlay_geometry(model: &AppModel, overlay: Overlay, area: Rect) -> OverlayGeometry {
    assert_ne!(
        overlay,
        Overlay::CommandPalette,
        "CommandPalette owns its geometry"
    );
    let desired_width = desired_width(overlay);
    let popup_width = desired_width.min(area.width.saturating_sub(4));
    let desired_height = desired_height(model, overlay, popup_width);
    let modal_area = modal_area(model, overlay, area);
    let popup_height = desired_height.min(modal_area.height);
    let popup = if super::is_composition_selector(overlay) {
        Rect::new(
            modal_area.x,
            modal_area.bottom().saturating_sub(popup_height),
            modal_area.width,
            popup_height,
        )
    } else {
        centered_popup(modal_area, popup_width, popup_height)
    };
    let inner = if super::is_composition_selector(overlay) {
        BottomPaneFrame::resolve(popup).inner()
    } else {
        ModalFrame::resolve(popup, overlay_padding(overlay)).inner()
    };
    let window = list_count_and_selection(model, overlay).map(|(count, selected)| {
        if matches!(overlay, Overlay::SessionPicker | Overlay::PromptHistory) {
            FilteredListGeometry::resolve(inner, count, selected).window
        } else {
            selection_window(count, selected, usize::from(inner.height.saturating_sub(3)))
        }
    });
    OverlayGeometry {
        popup,
        inner,
        window,
    }
}

fn modal_area(model: &AppModel, overlay: Overlay, area: Rect) -> Rect {
    if area.height < 12
        && (decision_sheet::project(model, overlay).is_some()
            || matches!(
                overlay,
                Overlay::SessionPicker
                    | Overlay::TurnNavigator
                    | Overlay::PromptHistory
                    | Overlay::Inspector
            ))
    {
        return area;
    }
    let transcript = FrameLayout::resolve(model, area).transcript;
    if transcript.height >= 8 {
        transcript
    } else {
        Rect::new(
            transcript.x,
            area.y,
            transcript.width,
            transcript.bottom().saturating_sub(area.y),
        )
    }
}

pub(super) fn overlay_padding(overlay: Overlay) -> Padding {
    match overlay {
        Overlay::CommandPalette => unreachable!("CommandPalette owns its padding"),
        Overlay::Inspector => Padding::new(1, 1, 0, 0),
        _ => Padding::new(2, 2, 1, 1),
    }
}

fn desired_width(overlay: Overlay) -> u16 {
    match overlay {
        Overlay::CommandPalette => unreachable!("CommandPalette owns its width"),
        Overlay::Help => 72,
        Overlay::TurnNavigator => 72,
        Overlay::Inspector => 62,
        Overlay::SessionPicker
        | Overlay::PromptHistory
        | Overlay::Suspension
        | Overlay::UnknownCommand
        | Overlay::AbandonConfirmation
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => 62,
    }
}

fn desired_height(model: &AppModel, overlay: Overlay, popup_width: u16) -> u16 {
    match overlay {
        Overlay::CommandPalette => unreachable!("CommandPalette owns its height"),
        Overlay::Help => 14,
        Overlay::SessionPicker => selector_height(model.matching_sessions().count(), 3),
        Overlay::PromptHistory => selector_height(model.matching_history().count(), 3),
        Overlay::TurnNavigator => selector_height(model.matching_landmark_indices().len(), 4),
        Overlay::Inspector => super::super::inspector::desired_height(model),
        Overlay::Suspension => decision_height(model, overlay, popup_width),
        Overlay::UnknownCommand
        | Overlay::AbandonConfirmation
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => decision_height(model, overlay, popup_width),
    }
}

fn selector_height(count: usize, chrome_rows: u16) -> u16 {
    u16::try_from(count.clamp(1, 8))
        .unwrap_or(u16::MAX)
        .saturating_add(chrome_rows)
}

fn decision_height(model: &AppModel, overlay: Overlay, popup_width: u16) -> u16 {
    let spec = decision_sheet::project(model, overlay).expect("decision overlay has a spec");
    let content_width = popup_width.saturating_sub(6).max(1);
    let rows = decision_sheet::layout(&spec, content_width, usize::MAX)
        .rows
        .len();
    u16::try_from(rows.saturating_add(4)).unwrap_or(u16::MAX)
}

pub(in crate::view) fn selection_at(
    model: &AppModel,
    overlay: Overlay,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    if overlay == Overlay::CommandPalette {
        return super::command_palette::selection_at(model, area, column, row);
    }
    let geometry = overlay_geometry(model, overlay, area);
    if overlay == Overlay::Inspector {
        return super::super::inspector::selection_at(model, geometry.popup, column, row);
    }
    if matches!(overlay, Overlay::SessionPicker | Overlay::PromptHistory) {
        let (count, selected) = list_count_and_selection(model, overlay)?;
        return FilteredListGeometry::resolve(geometry.inner, count, selected).selection_at(
            geometry.inner,
            column,
            row,
        );
    }
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
    if overlay == Overlay::CommandPalette {
        return super::command_palette::contains(model, area, column, row);
    }
    overlay_geometry(model, overlay, area)
        .popup
        .contains((column, row).into())
}

pub(in crate::view) fn decision_action_at(
    model: &AppModel,
    overlay: Overlay,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<ActionOverlayIntent> {
    let spec = decision_sheet::project(model, overlay)?;
    let geometry = overlay_geometry(model, overlay, area);
    let layout = decision_sheet::layout(
        &spec,
        geometry.inner.width,
        usize::from(geometry.inner.height),
    );
    let decision_sheet::DecisionRow::Actions(group) = layout
        .rows
        .get(usize::from(row.checked_sub(geometry.inner.y)?))?
    else {
        return None;
    };
    let mut x = geometry.inner.x.saturating_add(1);
    for (index, action) in group.iter().enumerate() {
        x = x.saturating_add(u16::from(index != 0) * 2);
        let width = u16::try_from(action.visual_key.width() + action.action.width() + 3).ok()?;
        if column >= x && column < x.saturating_add(width) {
            return Some(action.intent);
        }
        x = x.saturating_add(width);
    }
    None
}

pub(in crate::view) fn decision_choice_at(
    model: &AppModel,
    overlay: Overlay,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let spec = decision_sheet::project(model, overlay)?;
    let geometry = overlay_geometry(model, overlay, area);
    if column < geometry.inner.x || column >= geometry.inner.right() {
        return None;
    }
    let layout = decision_sheet::layout(
        &spec,
        geometry.inner.width,
        usize::from(geometry.inner.height),
    );
    match layout
        .rows
        .get(usize::from(row.checked_sub(geometry.inner.y)?))?
    {
        decision_sheet::DecisionRow::Choice { index, .. } => Some(*index),
        _ => None,
    }
}

fn list_count_and_selection(model: &AppModel, overlay: Overlay) -> Option<(usize, usize)> {
    match overlay {
        Overlay::CommandPalette => unreachable!("CommandPalette owns its result window"),
        Overlay::SessionPicker => {
            Some((model.matching_sessions().count(), model.session_selection))
        }
        Overlay::PromptHistory => Some((model.matching_history().count(), model.history_selection)),
        Overlay::TurnNavigator => Some((
            model.matching_landmark_indices().len(),
            model.turn_selection,
        )),
        Overlay::Inspector => None,
        _ => None,
    }
}
