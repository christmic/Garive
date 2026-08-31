use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::Widget,
};

use crate::{
    application::{AppModel, ExecutionState, FocusTarget},
    Theme,
};

use super::{palette, primitives::key_hints};

pub(super) fn render_footer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let colors = palette(theme);
    let hint = if let Some(notice) = model.notice.as_deref() {
        Line::from(vec![
            Span::styled(" ● ", colors.notice),
            Span::styled(notice, colors.normal),
        ])
    } else if model.composer.has_selection() {
        key_hints(&[("Alt+C", "copy selection")], colors)
    } else if model.command_suggestions_active() {
        key_hints(&[("Tab", "complete command")], colors)
    } else if model.composer.text().len() > 4_096 {
        Line::styled(
            format!(
                "  Message is {} bytes over the limit",
                model.composer.text().len() - 4_096
            ),
            colors.danger,
        )
    } else if model.composer.text().len() > 3_584 {
        Line::styled(
            format!("  {} of 4096 bytes", model.composer.text().len()),
            colors.warning,
        )
    } else {
        focus_hint(model, colors)
    };
    hint.render(area, buffer);
}

pub(super) fn is_visible(model: &AppModel) -> bool {
    model.notice.is_some()
        || model.composer.has_selection()
        || model.command_suggestions_active()
        || model.composer.text().len() > 3_584
        || model.execution == ExecutionState::Following
        || model.focus == FocusTarget::Conversation
}

fn focus_hint(model: &AppModel, colors: super::style::Palette) -> Line<'static> {
    let running = model.execution == ExecutionState::Following;
    match (model.focus, running, model.viewport.follow_latest) {
        (_, true, _) => key_hints(&[("Esc", "cancel run")], colors),
        (FocusTarget::Conversation, false, false) => key_hints(&[("End", "follow latest")], colors),
        (FocusTarget::Conversation, false, true) => {
            key_hints(&[("PgUp", "browse history")], colors)
        }
        _ => Line::default(),
    }
}
