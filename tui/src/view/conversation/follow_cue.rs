//! Detached-viewport status, action styling, and pointer geometry.

use ratatui::{buffer::Buffer, layout::Rect, text::Line, widgets::Widget};
use unicode_width::UnicodeWidthStr;

use crate::application::AppModel;

use super::super::style::Palette;

const SEPARATOR: &str = " · ";
const KEYCAP: &str = " End ";
const ACTION: &str = "follow latest";

struct FollowCue {
    status: String,
    action_visible: bool,
}

pub(super) fn render(model: &AppModel, colors: Palette, area: Rect, buffer: &mut Buffer) {
    let Some(cue) = project(model) else {
        return;
    };
    let mut line = Line::styled(cue.status, colors.badge);
    if cue.action_visible {
        line.push_span(ratatui::text::Span::styled(SEPARATOR, colors.muted));
        line.push_span(ratatui::text::Span::styled(KEYCAP, colors.keycap));
        line.push_span(ratatui::text::Span::styled(ACTION, colors.muted));
    }
    line.alignment(ratatui::layout::Alignment::Center)
        .render(Rect::new(area.x, area.y, area.width, 1), buffer);
}

pub(super) fn hit_test(model: &AppModel, area: Rect, column: u16, row: u16) -> bool {
    let Some(cue) = project(model).filter(|cue| cue.action_visible) else {
        return false;
    };
    if row != area.y || column < area.x || column >= area.right() {
        return false;
    }
    let width = cue
        .status
        .width()
        .saturating_add(SEPARATOR.width())
        .saturating_add(KEYCAP.width())
        .saturating_add(ACTION.width());
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    column >= x && column < x.saturating_add(width)
}

fn project(model: &AppModel) -> Option<FollowCue> {
    let status = if model.viewport.newer_updates > 0 {
        format!("↓ {} newer updates", model.viewport.newer_updates)
    } else if !model.viewport.follow_latest {
        "↑ Browsing history".into()
    } else {
        return None;
    };
    Some(FollowCue {
        status,
        action_visible: model.overlay.is_none(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Overlay;

    #[test]
    fn active_geometry_is_centered_and_overlay_removes_the_hit_target() {
        let mut model = AppModel::default();
        model.viewport.follow_latest = false;
        model.viewport.newer_updates = 12;
        let area = Rect::new(10, 4, 80, 12);
        let hits = (0..100)
            .filter(|column| hit_test(&model, area, *column, 4))
            .collect::<Vec<_>>();
        assert_eq!(hits.first().copied(), Some(30));
        assert_eq!(hits.last().copied(), Some(68));
        assert!(!hit_test(&model, area, 30, 5));

        model.overlay = Some(Overlay::Help);
        assert!(!(0..100).any(|column| hit_test(&model, area, column, 4)));
    }
}
