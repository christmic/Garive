//! Shared geometry for the conversation-first product frame.

use ratatui::layout::{Constraint, Layout, Rect};

use crate::application::AppModel;

use super::{composer, inspector, primitives::centered_column};

const STANDARD_TRANSCRIPT_WIDTH: u16 = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrameLayout {
    pub(super) context: Rect,
    pub(super) transcript: Rect,
    pub(super) composer: Rect,
    pub(super) hint: Rect,
    pub(super) inspector: Option<Rect>,
}

impl FrameLayout {
    pub(super) fn resolve(model: &AppModel, area: Rect) -> Self {
        let inspector = model
            .inspector
            .open
            .then(|| inspector::wide_area(area))
            .flatten();
        let content = if let Some(inspector) = inspector {
            let combined_width = area.width.min(129);
            let x = area.x + area.width.saturating_sub(combined_width) / 2;
            Rect::new(
                x,
                area.y,
                inspector.x.saturating_sub(x).saturating_sub(1),
                area.height,
            )
        } else if area.width >= 80 {
            centered_column(area, STANDARD_TRANSCRIPT_WIDTH)
        } else {
            area
        };
        let context_height = u16::from(content.height >= 10);
        // Keep the interactive surface stationary when contextual hints appear.
        // Below nine rows the hint is deliberately removed by the compact layout.
        let hint_height = u16::from(content.height >= 9);
        let body = Rect::new(
            content.x,
            content.y.saturating_add(context_height),
            content.width,
            content.height.saturating_sub(context_height),
        );
        let composer_height = if body.height < 11 {
            2
        } else {
            composer::desired_height(&model.composer, body.width).clamp(2, 6)
        };
        let rows = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(composer_height),
            Constraint::Length(hint_height),
        ])
        .split(body);
        Self {
            context: Rect::new(content.x, content.y, content.width, context_height),
            transcript: rows[0],
            composer: rows[1],
            hint: rows[2],
            inspector,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_hint_does_not_move_the_composer_hit_surface() {
        let mut model = AppModel::default();
        let area = Rect::new(0, 0, 100, 24);
        let quiet = FrameLayout::resolve(&model, area);

        model.notice = Some("Selection hint".into());
        let contextual = FrameLayout::resolve(&model, area);

        assert_eq!(quiet.composer, contextual.composer);
        assert_eq!(quiet.hint, contextual.hint);
        assert_eq!(quiet.hint.height, 1);
    }

    #[test]
    fn hint_row_is_removed_only_below_the_supported_height_breakpoint() {
        let model = AppModel::default();
        assert_eq!(
            FrameLayout::resolve(&model, Rect::new(0, 0, 40, 9))
                .hint
                .height,
            1
        );
        assert_eq!(
            FrameLayout::resolve(&model, Rect::new(0, 0, 40, 8))
                .hint
                .height,
            0
        );
    }
}
