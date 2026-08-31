//! Shared geometry for the conversation-first product frame.

use ratatui::layout::{Constraint, Layout, Rect};

use crate::application::AppModel;

use super::{composer, footer, primitives::centered_column};

const STANDARD_TRANSCRIPT_WIDTH: u16 = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrameLayout {
    pub(super) context: Rect,
    pub(super) transcript: Rect,
    pub(super) composer: Rect,
    pub(super) hint: Rect,
}

impl FrameLayout {
    pub(super) fn resolve(model: &AppModel, area: Rect) -> Self {
        let content = if area.width >= 80 {
            centered_column(area, STANDARD_TRANSCRIPT_WIDTH)
        } else {
            area
        };
        let context_height = u16::from(content.height >= 10);
        let hint_height = u16::from(content.height >= 9 && footer::is_visible(model));
        let body = Rect::new(
            content.x,
            content.y.saturating_add(context_height),
            content.width,
            content.height.saturating_sub(context_height),
        );
        let composer_height = if body.height < 11 {
            3
        } else {
            composer::desired_height(&model.composer, body.width).clamp(3, 7)
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
        }
    }
}
