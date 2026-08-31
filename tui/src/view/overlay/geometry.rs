use crate::application::ActionOverlayIntent;
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
    decision_sheet,
    layout::FrameLayout,
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
    let modal_area = modal_area(model, overlay, area);
    let popup = centered_popup(
        modal_area,
        popup_width,
        desired_height.min(modal_area.height),
    );
    let inner = Block::default()
        .borders(Borders::ALL)
        .padding(overlay_padding(overlay))
        .inner(popup);
    let window = list_count_and_selection(model, overlay).map(|(count, selected)| {
        let fixed_rows = if overlay == Overlay::CommandPalette {
            2
        } else {
            3
        };
        selection_window(
            count,
            selected,
            usize::from(inner.height.saturating_sub(fixed_rows)),
        )
    });
    OverlayGeometry {
        popup,
        inner,
        window,
    }
}

fn modal_area(model: &AppModel, overlay: Overlay, area: Rect) -> Rect {
    if decision_sheet::project(model, overlay).is_some() && area.height < 12 {
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
    if overlay == Overlay::CommandPalette {
        Padding::new(2, 2, 0, 1)
    } else {
        Padding::new(2, 2, 1, 1)
    }
}

fn desired_width(overlay: Overlay) -> u16 {
    match overlay {
        Overlay::CommandPalette => 74,
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
        Overlay::CommandPalette => u16::try_from(COMMAND_PALETTE.len())
            .unwrap_or(u16::MAX)
            .saturating_add(5)
            .clamp(10, 21),
        Overlay::Help => 14,
        Overlay::SessionPicker => u16::try_from(model.matching_sessions().count())
            .unwrap_or(u16::MAX)
            .saturating_add(7)
            .clamp(8, 16),
        Overlay::PromptHistory => u16::try_from(model.matching_history().count())
            .unwrap_or(u16::MAX)
            .saturating_add(7)
            .clamp(8, 16),
        Overlay::TurnNavigator => u16::try_from(model.matching_landmark_indices().len())
            .unwrap_or(u16::MAX)
            .saturating_add(7)
            .clamp(8, 18),
        Overlay::Inspector => u16::try_from(model.inspector_projection().entries.len())
            .unwrap_or(u16::MAX)
            .saturating_mul(2)
            .saturating_add(3)
            .clamp(8, 18),
        Overlay::Suspension => decision_height(model, overlay, popup_width),
        Overlay::UnknownCommand
        | Overlay::AbandonConfirmation
        | Overlay::ErrorDetails
        | Overlay::EphemeralConfirmation
        | Overlay::QuitConfirmation => decision_height(model, overlay, popup_width),
    }
}

fn decision_height(model: &AppModel, overlay: Overlay, popup_width: u16) -> u16 {
    let spec = decision_sheet::project(model, overlay).expect("decision overlay has a spec");
    let content_width = popup_width.saturating_sub(6).max(1);
    let body_rows = spec
        .body
        .iter()
        .map(|line| wrapped_rows(&safe_text(line), content_width))
        .sum::<usize>();
    let fixed =
        5 + spec.response.as_ref().map_or(0, |response| match response {
            decision_sheet::DecisionResponseSpec::Editor { .. } => 4,
            decision_sheet::DecisionResponseSpec::Choices { choices, .. } => 3 + choices.len(),
            decision_sheet::DecisionResponseSpec::ReadOnly { .. } => 3,
        }) + usize::from(!spec.actions.is_empty()) * 2;
    u16::try_from(body_rows.saturating_add(fixed)).unwrap_or(u16::MAX)
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
    if overlay == Overlay::Inspector {
        return super::super::inspector::selection_at(model, geometry.popup, column, row);
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
    let geometry = overlay_geometry(model, overlay, area);
    let actions = decision_sheet::project(model, overlay)?.actions;
    let groups = decision_sheet::action_groups(&actions, geometry.inner.width);
    let first_row = geometry
        .inner
        .bottom()
        .saturating_sub(u16::try_from(groups.len()).ok()?);
    let group = groups.get(usize::from(row.checked_sub(first_row)?))?;
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
    let geometry = overlay_geometry(model, overlay, area);
    if column < geometry.inner.x || column >= geometry.inner.right() {
        return None;
    }
    let spec = decision_sheet::project(model, overlay)?;
    let super::super::decision_sheet::DecisionResponseSpec::Choices {
        choices, selected, ..
    } = spec.response?
    else {
        return None;
    };
    let full_rows = 1 + spec.body.len() + choices.len() + 3 + 1;
    if full_rows > usize::from(geometry.inner.height) {
        return (row == geometry.inner.y.saturating_add(1)).then_some(selected);
    }
    let first = geometry
        .inner
        .y
        .saturating_add(1 + u16::try_from(spec.body.len()).ok()?);
    (row >= first && row < first.saturating_add(u16::try_from(choices.len()).ok()?))
        .then(|| usize::from(row - first))
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
        Overlay::TurnNavigator => Some((
            model.matching_landmark_indices().len(),
            model.turn_selection,
        )),
        Overlay::Inspector => None,
        _ => None,
    }
}
