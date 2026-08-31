use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    application::AppModel,
    input::COMMAND_PALETTE,
    view::{
        primitives::{selection_window, truncate_display},
        style::Palette,
    },
};

const MAX_VISIBLE_ROWS: usize = 5;
const MAX_WIDTH: u16 = 76;

pub(super) fn render(model: &AppModel, composer: Rect, colors: Palette, buffer: &mut Buffer) {
    let Some(area) = area(model, composer) else {
        return;
    };
    let matches = model.matching_command_suggestion_indices();
    let (start, end) = selection_window(
        matches.len(),
        model.command_suggestion_selection,
        MAX_VISIBLE_ROWS,
    );
    Clear.render(area, buffer);
    let block = Block::default()
        .title(Line::styled(" Commands ", colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(colors.overlay_border)
        .padding(Padding::horizontal(1));
    let inner = inner_area(area);
    block.render(area, buffer);
    for (row, match_index) in matches[start..end].iter().enumerate() {
        let command = COMMAND_PALETTE[*match_index];
        let selected = start + row == model.command_suggestion_selection;
        let marker = if selected { "› " } else { "  " };
        let reason = command.unavailable_reason(model.command_context());
        let detail = reason
            .map(|value| format!("unavailable · {value}"))
            .unwrap_or_else(|| command.help.to_owned());
        let fixed_width = UnicodeWidthStr::width(marker)
            + UnicodeWidthStr::width(command.input)
            + UnicodeWidthStr::width("  ");
        let detail = truncate_display(
            &detail,
            usize::from(inner.width).saturating_sub(fixed_width),
        );
        let line = Line::from(vec![
            Span::styled(
                marker,
                if selected {
                    colors.selected
                } else {
                    colors.muted
                },
            ),
            Span::styled(command.input, colors.accent),
            Span::styled(if detail.is_empty() { "" } else { "  " }, colors.muted),
            Span::styled(
                detail,
                if reason.is_some() {
                    colors.warning
                } else {
                    colors.muted
                },
            ),
        ]);
        Paragraph::new(line)
            .style(if selected {
                colors.selection_row
            } else {
                colors.normal
            })
            .render(
                Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                buffer,
            );
    }
}

pub(super) fn area(model: &AppModel, composer: Rect) -> Option<Rect> {
    if model.composer_is_frozen || !model.command_suggestions_active() {
        return None;
    }
    let rows = model
        .matching_command_suggestion_indices()
        .len()
        .min(MAX_VISIBLE_ROWS) as u16;
    let height = rows + 2;
    let width = composer.width.min(MAX_WIDTH);
    Some(Rect::new(
        composer.x,
        composer.y.saturating_sub(height),
        width,
        height,
    ))
}

pub(crate) fn selection_at(
    model: &AppModel,
    composer: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let area = area(model, composer)?;
    let inner = inner_area(area);
    if column < inner.x
        || column >= inner.x.saturating_add(inner.width)
        || row < inner.y
        || row >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let matches = model.matching_command_suggestion_indices();
    let (start, end) = selection_window(
        matches.len(),
        model.command_suggestion_selection,
        MAX_VISIBLE_ROWS,
    );
    let index = start + usize::from(row - inner.y);
    (index < end).then_some(index)
}

fn inner_area(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(1),
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{FocusTarget, TerminalSize};

    #[test]
    fn hit_test_only_returns_visible_rows() {
        let mut model = AppModel {
            focus: FocusTarget::Composer,
            terminal_size: TerminalSize {
                width: 100,
                height: 24,
            },
            ..Default::default()
        };
        model.composer.replace("/").unwrap();
        model.command_suggestion_selection = 8;
        let composer = Rect::new(28, 20, 72, 3);
        let popup = area(&model, composer).unwrap();
        assert_eq!(
            selection_at(&model, composer, popup.x + 2, popup.y + 1),
            Some(4)
        );
        assert_eq!(
            selection_at(&model, composer, popup.x + 1, popup.y + 1),
            None
        );
        assert_eq!(selection_at(&model, composer, popup.x, popup.y), None);
    }

    #[test]
    fn row_details_truncate_by_display_width_without_splitting_graphemes() {
        assert_eq!(
            truncate_display("Follow terminal theme", 12),
            "Follow term…"
        );
        assert_eq!(truncate_display("状态：你好世界", 9), "状态：你…");
        assert_eq!(
            UnicodeWidthStr::width(truncate_display("状态：你好世界", 9).as_str()),
            9
        );
        assert_eq!(truncate_display("ignored", 1), "…");
        assert_eq!(truncate_display("ignored", 0), "");
    }
}
