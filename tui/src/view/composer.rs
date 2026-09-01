use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    application::{AppModel, ExecutionState, FocusTarget},
    input::EditorState,
};

use super::{composer_run_rail, safe_text, style::Palette, MotionFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComposerVariant {
    Idle,
    Focused,
    Frozen,
    ActionResponse,
}

pub(super) fn render(
    model: &AppModel,
    colors: Palette,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
) {
    let variant = variant(model);
    let status = status_line(model, variant, colors, motion);
    let dock = ComposerDock::resolve(area, status.is_some());
    dock.render_status(status, colors, buffer);
    Clear.render(dock.body, buffer);
    let marker_style = match variant {
        ComposerVariant::Focused => colors.accent,
        ComposerVariant::Frozen => colors.warning,
        ComposerVariant::ActionResponse => colors.notice,
        ComposerVariant::Idle => colors.muted,
    };
    Paragraph::new(Line::styled("› ", marker_style))
        .render(Rect::new(dock.body.x, dock.body.y, 2, 1), buffer);
    let text = if model.composer.text().is_empty() {
        let placeholder = if variant == ComposerVariant::Frozen {
            "Draft retained · waiting for durable truth"
        } else if model.execution == ExecutionState::Following {
            "Draft while current Turn runs"
        } else {
            "Ask Garive anything"
        };
        Text::from(Line::styled(placeholder, colors.placeholder))
    } else {
        EditorLayout::new(&model.composer, dock.content.width).text(colors)
    };
    let (_, cursor_scroll) =
        EditorLayout::new(&model.composer, dock.content.width).visible_cursor(dock.content.height);
    // A frozen Composer is evidence, not an editing viewport. Keep its beginning
    // visible so compact layouts never degrade into a meaningless trailing word.
    let scroll = if variant == ComposerVariant::Frozen {
        0
    } else {
        cursor_scroll
    };
    Paragraph::new(text)
        .style(colors.normal)
        .scroll((scroll, 0))
        .render(dock.content, buffer);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ComposerDock {
    status: Rect,
    body: Rect,
    content: Rect,
}

impl ComposerDock {
    fn resolve(area: Rect, has_status: bool) -> Self {
        let status_height = if area.height <= 1 {
            0
        } else if has_status {
            area.height.saturating_sub(1).min(3)
        } else {
            1
        };
        let status = Rect::new(area.x, area.y, area.width, status_height);
        let body = Rect::new(
            area.x,
            area.y.saturating_add(status_height),
            area.width,
            area.height.saturating_sub(status_height),
        );
        let content = Rect::new(
            body.x.saturating_add(2),
            body.y,
            body.width.saturating_sub(2),
            body.height,
        );
        Self {
            status,
            body,
            content,
        }
    }

    fn render_status(self, line: Option<Line<'static>>, colors: Palette, buffer: &mut Buffer) {
        Clear.render(self.status, buffer);
        if let Some(line) = line {
            Paragraph::new(line)
                .style(colors.normal)
                .render(self.status, buffer);
        }
    }
}

fn status_line(
    model: &AppModel,
    variant: ComposerVariant,
    colors: Palette,
    motion: MotionFrame,
) -> Option<Line<'static>> {
    let variant_line = match variant {
        ComposerVariant::Frozen => Some(Line::from(vec![
            Span::styled("! ", colors.warning),
            Span::styled("Draft locked", colors.title),
            Span::styled(" · read only", colors.muted),
        ])),
        ComposerVariant::ActionResponse => Some(Line::from(vec![
            Span::styled("! ", colors.notice),
            Span::styled("Action response", colors.title),
        ])),
        ComposerVariant::Idle | ComposerVariant::Focused => None,
    };
    if composer_run_rail::has_cancel_request(model) {
        composer_run_rail::line(model, colors, motion)
    } else {
        variant_line.or_else(|| composer_run_rail::line(model, colors, motion))
    }
}

fn has_status(model: &AppModel, variant: ComposerVariant) -> bool {
    matches!(
        variant,
        ComposerVariant::Frozen | ComposerVariant::ActionResponse
    ) || composer_run_rail::visible(model)
}

fn variant(model: &AppModel) -> ComposerVariant {
    if model.composer_is_frozen {
        ComposerVariant::Frozen
    } else if model.execution == ExecutionState::Suspended {
        ComposerVariant::ActionResponse
    } else if model.focus == FocusTarget::Composer {
        ComposerVariant::Focused
    } else {
        ComposerVariant::Idle
    }
}

pub(super) fn cursor(model: &AppModel, area: Rect) -> Option<(u16, u16)> {
    if model.composer_is_frozen {
        return None;
    }
    let content = ComposerDock::resolve(area, has_status(model, variant(model))).content;
    if content.is_empty() {
        return None;
    }
    let ((column, row), scroll) =
        EditorLayout::new(&model.composer, content.width).visible_cursor(content.height);
    Some((content.x + column, content.y + row.saturating_sub(scroll)))
}

pub(super) fn desired_height(model: &AppModel, area_width: u16, roomy: bool) -> u16 {
    let inner_width = area_width.saturating_sub(2);
    if inner_width == 0 {
        return 2;
    }
    let layout = EditorLayout::new(&model.composer, inner_width);
    let ((_, cursor_row), _) = layout.visible_cursor(u16::MAX);
    let rows = u16::try_from(layout.rows.len()).unwrap_or(u16::MAX);
    let status_rows = if roomy && has_status(model, variant(model)) {
        3
    } else {
        1
    };
    rows.max(cursor_row.saturating_add(1))
        .saturating_add(status_rows)
}

pub(super) fn selection_at(
    model: &AppModel,
    area: Rect,
    column: u16,
    row: u16,
    clamp: bool,
) -> Option<usize> {
    if model.composer_is_frozen {
        return None;
    }
    let inner = ComposerDock::resolve(area, has_status(model, variant(model))).content;
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
mod tests;
