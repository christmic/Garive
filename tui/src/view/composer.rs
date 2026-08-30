use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
        EditorLayout::new(&model.composer, inner.width).text(colors)
    };
    let (_, scroll) = EditorLayout::new(&model.composer, inner.width).visible_cursor(inner.height);
    Paragraph::new(text)
        .style(colors.normal)
        .scroll((scroll, 0))
        .render(inner, buffer);
}

pub(super) fn cursor(model: &AppModel, area: Rect) -> Option<(u16, u16)> {
    let inner_width = area.width.saturating_sub(4);
    let inner_height = area.height.saturating_sub(2);
    if inner_width == 0 || inner_height == 0 {
        return None;
    }
    let ((column, row), scroll) =
        EditorLayout::new(&model.composer, inner_width).visible_cursor(inner_height);
    Some((area.x + 2 + column, area.y + 1 + row.saturating_sub(scroll)))
}

pub(super) fn desired_height(editor: &EditorState, area_width: u16) -> u16 {
    let inner_width = area_width.saturating_sub(4);
    if inner_width == 0 {
        return 3;
    }
    let layout = EditorLayout::new(editor, inner_width);
    let ((_, cursor_row), _) = layout.visible_cursor(u16::MAX);
    let rows = u16::try_from(layout.rows.len()).unwrap_or(u16::MAX);
    rows.max(cursor_row.saturating_add(1)).saturating_add(2)
}

pub(super) fn selection_at(
    model: &AppModel,
    area: Rect,
    column: u16,
    row: u16,
    clamp: bool,
) -> Option<usize> {
    let inner = Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(1),
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    if inner.is_empty() || (!clamp && !inner.contains((column, row).into())) {
        return None;
    }
    let column = column
        .saturating_sub(inner.x)
        .min(inner.width.saturating_sub(1));
    let visible_row = row
        .saturating_sub(inner.y)
        .min(inner.height.saturating_sub(1));
    let layout = EditorLayout::new(&model.composer, inner.width);
    let (_, scroll) = layout.visible_cursor(inner.height);
    Some(layout.grapheme_at(column, visible_row.saturating_add(scroll)))
}

#[derive(Clone)]
struct LayoutToken {
    grapheme: usize,
    value: String,
    width: u16,
    selected: bool,
}

struct LayoutRow {
    start: usize,
    end: usize,
    tokens: Vec<LayoutToken>,
}

struct EditorLayout {
    width: u16,
    cursor: usize,
    rows: Vec<LayoutRow>,
}

impl EditorLayout {
    fn new(editor: &EditorState, width: u16) -> Self {
        let width = width.max(1);
        let selection = editor.selected_byte_range();
        let mut rows = Vec::new();
        let mut logical = Vec::new();
        let mut logical_start = 0;
        let mut grapheme_count = 0;
        for (grapheme, (byte, value)) in editor.text().grapheme_indices(true).enumerate() {
            grapheme_count = grapheme + 1;
            if value == "\n" {
                wrap_logical_line(&logical, logical_start, grapheme, width, &mut rows);
                logical.clear();
                logical_start = grapheme + 1;
                continue;
            }
            let value = safe_text(value);
            logical.push(LayoutToken {
                grapheme,
                width: UnicodeWidthStr::width(value.as_str()).min(u16::MAX as usize) as u16,
                selected: selection.is_some_and(|(start, end)| byte >= start && byte < end),
                value,
            });
        }
        wrap_logical_line(&logical, logical_start, grapheme_count, width, &mut rows);
        Self {
            width,
            cursor: editor.cursor_grapheme(),
            rows,
        }
    }

    fn text(&self, colors: Palette) -> Text<'static> {
        Text::from(
            self.rows
                .iter()
                .map(|row| {
                    let mut segments = Vec::<(String, bool)>::new();
                    for token in &row.tokens {
                        if let Some((tail, _)) =
                            segments.last_mut().filter(|item| item.1 == token.selected)
                        {
                            tail.push_str(&token.value);
                        } else {
                            segments.push((token.value.clone(), token.selected));
                        }
                    }
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

    fn visible_cursor(&self, height: u16) -> ((u16, u16), u16) {
        let position = self.position_for(self.cursor);
        let scroll = position.1.saturating_sub(height.saturating_sub(1));
        (position, scroll)
    }

    fn position_for(&self, cursor: usize) -> (u16, u16) {
        for (row_index, row) in self.rows.iter().enumerate() {
            let next_start = self.rows.get(row_index + 1).map(|next| next.start);
            if cursor >= row.start
                && (cursor < row.end || (cursor == row.end && next_start != Some(cursor)))
            {
                let column = row
                    .tokens
                    .iter()
                    .take_while(|token| token.grapheme < cursor)
                    .map(|token| token.width)
                    .sum::<u16>();
                return if column >= self.width {
                    (0, row_index.saturating_add(1) as u16)
                } else {
                    (column, row_index as u16)
                };
            }
        }
        (0, 0)
    }

    fn grapheme_at(&self, column: u16, row: u16) -> usize {
        let Some(line) = self.rows.get(usize::from(row)) else {
            return self.rows.last().map_or(0, |line| line.end);
        };
        let mut used = 0;
        for token in &line.tokens {
            let next = used + token.width;
            if column < next {
                if token.width > 1 && (column - used) * 2 >= token.width {
                    return token.grapheme + 1;
                }
                return token.grapheme;
            }
            used = next;
        }
        line.end
    }

    fn vertical_target(
        &self,
        origin: usize,
        preferred_column: Option<usize>,
        direction: i8,
    ) -> (usize, usize) {
        let (current_column, current_row) = self.position_for(origin);
        let preferred = preferred_column.unwrap_or(usize::from(current_column));
        let visual_rows = self.visual_row_count();
        let target_row = if direction < 0 {
            current_row.saturating_sub(1)
        } else {
            current_row
                .saturating_add(1)
                .min(visual_rows.saturating_sub(1))
        };
        if target_row == current_row {
            return (origin, preferred);
        }
        let target = self.vertical_grapheme_at(preferred, target_row);
        (target, preferred)
    }

    fn line_edge_target(&self, origin: usize, direction: i8) -> usize {
        let (_, row) = self.position_for(origin);
        let Some(line) = self.rows.get(usize::from(row)) else {
            return self.rows.last().map_or(0, |line| line.end);
        };
        if direction < 0 {
            return line.start;
        }
        if self
            .rows
            .get(usize::from(row).saturating_add(1))
            .is_some_and(|next| next.start == line.end)
        {
            line.tokens
                .last()
                .map_or(line.start, |token| token.grapheme)
        } else {
            line.end
        }
    }

    fn visual_row_count(&self) -> u16 {
        let rows = u16::try_from(self.rows.len()).unwrap_or(u16::MAX);
        let continuation = self.rows.last().is_some_and(|row| {
            row.tokens.iter().map(|token| token.width).sum::<u16>() >= self.width
        });
        rows.saturating_add(u16::from(continuation))
    }

    fn vertical_grapheme_at(&self, column: usize, row: u16) -> usize {
        let Some(line) = self.rows.get(usize::from(row)) else {
            return self.rows.last().map_or(0, |line| line.end);
        };
        let mut used = 0_usize;
        for token in &line.tokens {
            let next = used.saturating_add(usize::from(token.width));
            if column < next {
                return token.grapheme;
            }
            used = next;
        }
        if self
            .rows
            .get(usize::from(row).saturating_add(1))
            .is_some_and(|next| next.start == line.end)
        {
            line.tokens
                .last()
                .map_or(line.start, |token| token.grapheme)
        } else {
            line.end
        }
    }
}

pub(super) fn vertical_target(editor: &EditorState, width: u16, direction: i8) -> (usize, usize) {
    let (origin, preferred) = editor.visual_vertical_state(direction);
    EditorLayout::new(editor, width.max(1)).vertical_target(origin, preferred, direction)
}

pub(super) fn line_edge_target(editor: &EditorState, width: u16, direction: i8) -> usize {
    let origin = editor.visual_directional_origin(direction);
    EditorLayout::new(editor, width.max(1)).line_edge_target(origin, direction)
}

fn wrap_logical_line(
    tokens: &[LayoutToken],
    logical_start: usize,
    logical_end: usize,
    width: u16,
    rows: &mut Vec<LayoutRow>,
) {
    if tokens.is_empty() {
        rows.push(LayoutRow {
            start: logical_start,
            end: logical_end,
            tokens: Vec::new(),
        });
        return;
    }
    let mut start = 0;
    while start < tokens.len() {
        let mut used: u16 = 0;
        let mut end = start;
        let mut last_break = None;
        while end < tokens.len() {
            let token = &tokens[end];
            if used.saturating_add(token.width) > width && end > start {
                break;
            }
            used = used.saturating_add(token.width);
            end += 1;
            if token.value.chars().all(char::is_whitespace) {
                last_break = Some(end);
            }
            if used >= width {
                break;
            }
        }
        if end < tokens.len() {
            end = last_break.filter(|value| *value > start).unwrap_or(end);
        }
        let row_start = tokens[start].grapheme;
        let row_end = tokens.get(end).map_or(logical_end, |token| token.grapheme);
        rows.push(LayoutRow {
            start: row_start,
            end: row_end,
            tokens: tokens[start..end].to_vec(),
        });
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{view::style::palette, Theme};

    #[test]
    fn word_wrap_and_cursor_share_the_same_rows() {
        let mut editor = EditorState::new(128);
        editor.replace("hello world").unwrap();
        let layout = EditorLayout::new(&editor, 8);

        let lines = layout
            .text(palette(Theme::Mono))
            .lines
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>();
        assert_eq!(lines, ["hello ", "world"]);
        assert_eq!(layout.visible_cursor(3), ((5, 1), 0));
    }

    #[test]
    fn wrapped_selection_preserves_graphemes_and_semantic_style() {
        let mut editor = EditorState::new(128);
        editor.replace("ab 界e\u{301} cd").unwrap();
        editor.move_document_start(false);
        for _ in 0..5 {
            editor.move_right(true);
        }
        let colors = palette(Theme::Mono);
        let text = EditorLayout::new(&editor, 5).text(colors);

        assert_eq!(text.lines.len(), 3);
        let selected = text
            .lines
            .iter()
            .flat_map(|line| &line.spans)
            .filter(|span| span.style == colors.text_selection)
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert_eq!(selected, "ab 界e\u{301}");
    }

    #[test]
    fn exact_width_cursor_advances_to_a_visible_continuation_row() {
        let mut editor = EditorState::new(128);
        editor.replace("12345").unwrap();
        let layout = EditorLayout::new(&editor, 5);

        assert_eq!(layout.visible_cursor(2), ((0, 1), 0));
    }

    #[test]
    fn explicit_newline_and_word_wrap_share_cursor_geometry() {
        let mut editor = EditorState::new(128);
        editor.replace("one\ntwo three").unwrap();
        let layout = EditorLayout::new(&editor, 6);

        let lines = layout
            .text(palette(Theme::Mono))
            .lines
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>();
        assert_eq!(lines, ["one", "two ", "three"]);
        assert_eq!(layout.visible_cursor(2), ((5, 2), 1));
    }

    #[test]
    fn pointer_hit_testing_uses_wrapped_grapheme_boundaries() {
        let mut editor = EditorState::new(128);
        editor.replace("ab 界e\u{301} cd").unwrap();
        let layout = EditorLayout::new(&editor, 5);

        assert_eq!(layout.grapheme_at(0, 0), 0);
        assert_eq!(layout.grapheme_at(4, 0), 3);
        assert_eq!(layout.grapheme_at(0, 1), 3);
        assert_eq!(layout.grapheme_at(1, 1), 4);
        assert_eq!(layout.grapheme_at(4, 1), 6);
        assert_eq!(layout.grapheme_at(0, 9), 8);
    }

    #[test]
    fn desired_height_counts_visual_rows_and_exact_width_cursor() {
        let mut editor = EditorState::new(128);
        assert_eq!(desired_height(&editor, 12), 3);

        editor.replace("hello world").unwrap();
        assert_eq!(desired_height(&editor, 12), 4);

        editor.replace("12345").unwrap();
        assert_eq!(desired_height(&editor, 9), 4);
    }

    #[test]
    fn visual_vertical_navigation_uses_wrapped_rows_and_sticky_column() {
        let mut editor = EditorState::new(128);
        editor.replace("hello world").unwrap();

        let (target, preferred) = vertical_target(&editor, 8, -1);
        assert_eq!((target, preferred), (5, 5));
        editor.apply_visual_vertical_move(target, preferred, -1, false);
        assert_eq!(editor.cursor_grapheme(), 5);

        let (target, preferred) = vertical_target(&editor, 8, 1);
        assert_eq!((target, preferred), (11, 5));
        editor.apply_visual_vertical_move(target, preferred, 1, false);
        assert_eq!(editor.cursor_grapheme(), 11);
    }

    #[test]
    fn visual_vertical_navigation_handles_wide_cells_and_continuation_rows() {
        let mut editor = EditorState::new(128);
        editor.replace("ab界cd").unwrap();
        let (target, preferred) = vertical_target(&editor, 4, -1);
        assert_eq!((target, preferred), (2, 2));

        editor.replace("12345").unwrap();
        let (target, preferred) = vertical_target(&editor, 5, -1);
        assert_eq!((target, preferred), (0, 0));
        editor.apply_visual_vertical_move(target, preferred, -1, false);
        let (target, preferred) = vertical_target(&editor, 5, 1);
        assert_eq!((target, preferred), (5, 0));
    }

    #[test]
    fn visual_line_edges_follow_soft_wraps_and_exact_width_continuation() {
        let mut editor = EditorState::new(128);
        editor.replace("hello world").unwrap();
        editor.move_document_start(false);
        assert_eq!(line_edge_target(&editor, 8, 1), 5);
        editor.place_cursor(8, false);
        assert_eq!(line_edge_target(&editor, 8, -1), 6);
        assert_eq!(line_edge_target(&editor, 8, 1), 11);
        editor.move_document_end(false);
        editor.move_left(true);
        editor.move_left(true);
        assert_eq!(line_edge_target(&editor, 8, -1), 6);
        assert_eq!(line_edge_target(&editor, 8, 1), 11);

        editor.replace("12345").unwrap();
        assert_eq!(line_edge_target(&editor, 5, -1), 5);
        assert_eq!(line_edge_target(&editor, 5, 1), 5);
    }
}
