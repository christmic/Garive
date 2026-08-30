use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Widget, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    application::{AppModel, ExecutionState, FocusTarget},
    input::EditorState,
};

use super::{safe_text, style::Palette};

pub(super) fn render(model: &AppModel, colors: Palette, area: Rect, buffer: &mut Buffer) {
    let title = if model.execution == ExecutionState::Suspended {
        " Action response "
    } else {
        " Compose "
    };
    let block = Block::default()
        .title(Line::styled(title, colors.title))
        .borders(Borders::ALL)
        .border_type(if model.focus == FocusTarget::Composer {
            BorderType::Double
        } else {
            BorderType::Rounded
        })
        .border_style(if model.focus == FocusTarget::Composer {
            colors.composer_border
        } else {
            colors.border
        })
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buffer);
    let text = if model.composer.text().is_empty() {
        Text::from(Line::styled(
            "›  Message Garive — / for commands",
            colors.placeholder,
        ))
    } else {
        editor_text(&model.composer, colors)
    };
    let (_, scroll) = visual_cursor(model, inner.width, inner.height);
    Paragraph::new(text)
        .style(colors.normal)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .render(inner, buffer);
}

pub(super) fn cursor(model: &AppModel, area: Rect) -> Option<(u16, u16)> {
    let inner_width = area.width.saturating_sub(4);
    let inner_height = area.height.saturating_sub(2);
    if inner_width == 0 || inner_height == 0 {
        return None;
    }
    let ((column, row), scroll) = visual_cursor(model, inner_width, inner_height);
    Some((area.x + 2 + column, area.y + 1 + row.saturating_sub(scroll)))
}

fn editor_text(editor: &EditorState, colors: Palette) -> Text<'static> {
    let selection = editor.selected_byte_range();
    let mut lines = vec![Vec::<(String, bool)>::new()];
    for (byte, grapheme) in editor.text().grapheme_indices(true) {
        if grapheme == "\n" {
            lines.push(Vec::new());
            continue;
        }
        let selected = selection.is_some_and(|(start, end)| byte >= start && byte < end);
        let value = safe_text(grapheme);
        let line = lines.last_mut().expect("composer always has one line");
        if let Some((tail, _)) = line.last_mut().filter(|item| item.1 == selected) {
            tail.push_str(&value);
        } else {
            line.push((value, selected));
        }
    }
    Text::from(
        lines
            .into_iter()
            .map(|segments| {
                Line::from(
                    segments
                        .into_iter()
                        .map(|(value, selected)| {
                            Span::styled(
                                value,
                                if selected {
                                    colors.text_selection
                                } else {
                                    colors.normal
                                },
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn visual_cursor(model: &AppModel, width: u16, height: u16) -> ((u16, u16), u16) {
    let width = width.max(1);
    let lines_before = model
        .composer
        .text()
        .lines()
        .take(model.composer.cursor_line())
        .map(|line| {
            let columns = unicode_width::UnicodeWidthStr::width(line) as u16;
            columns.max(1).div_ceil(width)
        })
        .sum::<u16>();
    let display_column = model.composer.display_column().min(u16::MAX as usize) as u16;
    let row = lines_before.saturating_add(display_column / width);
    let column = display_column % width;
    let scroll = row.saturating_sub(height.saturating_sub(1));
    ((column, row), scroll)
}
