//! Safe product view below the 40-column composition boundary.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget},
};

use crate::application::{AppModel, ExecutionState};

use super::style::Palette;

pub(super) fn render(model: &AppModel, colors: Palette, area: Rect, buffer: &mut Buffer) {
    let mut lines = vec![Line::styled("Garive needs 40 columns", colors.title)];
    lines.push(Line::styled(
        if model.composer.text().is_empty() {
            "Resize the terminal to continue"
        } else {
            "Resize terminal · draft retained"
        },
        colors.muted,
    ));
    if model.execution == ExecutionState::Following {
        lines.push(Line::styled("Run continues · Esc cancel", colors.warning));
    }
    Paragraph::new(lines).render(area, buffer);
}
