use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Widget,
};

use crate::{
    application::{AppModel, ExecutionState, FocusTarget},
    Theme,
};

use super::{palette, primitives::key_hints};

pub(super) fn render_footer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    let colors = palette(theme);
    let cells = Layout::horizontal([Constraint::Min(1), Constraint::Length(14)]).split(area);
    let hint = if let Some(notice) = model.notice.as_deref() {
        Line::from(vec![
            Span::styled(" ● ", colors.notice),
            Span::styled(notice, colors.normal),
        ])
    } else {
        focus_hints(model, area.width, colors)
    };
    hint.render(cells[0], buffer);
    Line::styled(
        format!("{} / 4096 B ", model.composer.text().len()),
        colors.muted,
    )
    .alignment(Alignment::Right)
    .render(cells[1], buffer);
}

fn focus_hints(model: &AppModel, width: u16, colors: super::style::Palette) -> Line<'static> {
    let running = model.execution == ExecutionState::Following;
    match (width < 60, model.focus, running) {
        (true, FocusTarget::Conversation, _) => {
            key_hints(&[("PgUp/PgDn", "scroll"), ("Tab", "compose")], colors)
        }
        (true, _, true) => key_hints(&[("Esc", "cancel"), ("?", "help")], colors),
        (true, _, false) => key_hints(&[("Enter", "send"), ("?", "help")], colors),
        (false, FocusTarget::Navigation, true) => key_hints(
            &[
                ("Esc", "cancel"),
                ("↑/↓", "select"),
                ("Enter", "open"),
                ("Tab", "next"),
            ],
            colors,
        ),
        (false, FocusTarget::Navigation, false) => key_hints(
            &[
                ("↑/↓", "select"),
                ("Enter", "open"),
                ("Tab", "next"),
                ("?", "help"),
            ],
            colors,
        ),
        (false, FocusTarget::Conversation, true) => key_hints(
            &[
                ("Esc", "cancel"),
                ("PgUp/PgDn", "scroll"),
                ("End", "latest"),
            ],
            colors,
        ),
        (false, FocusTarget::Conversation, false) => key_hints(
            &[
                ("PgUp/PgDn", "scroll"),
                ("End", "latest"),
                ("Tab", "compose"),
                ("?", "help"),
            ],
            colors,
        ),
        (false, _, true) => key_hints(
            &[
                ("Esc", "cancel"),
                ("Ctrl+S", "sessions"),
                ("Ctrl+P", "commands"),
                ("?", "help"),
            ],
            colors,
        ),
        (false, _, false) => key_hints(
            &[
                ("Enter", "send"),
                ("Ctrl+J", "newline"),
                ("Ctrl+P", "commands"),
                ("?", "help"),
            ],
            colors,
        ),
    }
}
