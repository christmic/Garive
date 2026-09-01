use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{application::AppModel, input::COMMAND_PALETTE};

use super::super::{
    layout::FrameLayout,
    primitives::{key_hints, selection_window, BottomPaneFrame, SelectionRow},
    safe_text,
    style::Palette,
};

const COMPACT_WIDTH: u16 = 50;
const MAX_VISIBLE_ITEMS: usize = 8;

mod highlight;
use highlight::highlighted_field;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PaletteItem {
    input: &'static str,
    help: &'static str,
    detail: String,
    unavailable_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PaletteProjection<'a> {
    query: &'a str,
    items: Vec<PaletteItem>,
    selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PaletteLayout {
    popup: Rect,
    inner: Rect,
    window: (usize, usize),
    first_item_row: u16,
    action_row: u16,
    compact: bool,
}

impl PaletteLayout {
    #[cfg(test)]
    fn item_capacity(self) -> usize {
        usize::from(self.action_row.saturating_sub(self.first_item_row))
    }

    fn visible_count(self) -> usize {
        self.window.1.saturating_sub(self.window.0)
    }
}

fn project(model: &AppModel) -> PaletteProjection<'_> {
    let context = model.command_context();
    let items = model
        .matching_command_indices()
        .into_iter()
        .map(|command_index| {
            let command = COMMAND_PALETTE[command_index];
            let reason = command.unavailable_reason(context);
            PaletteItem {
                input: command.input,
                help: command.help,
                detail: reason
                    .map(|value| format!("unavailable · {value}"))
                    .unwrap_or_else(|| command.help.to_owned()),
                unavailable_reason: reason,
            }
        })
        .collect::<Vec<_>>();
    let selected = model.command_selection.min(items.len().saturating_sub(1));
    PaletteProjection {
        query: &model.command_filter,
        items,
        selected,
    }
}

fn layout(model: &AppModel, area: Rect) -> PaletteLayout {
    let projection = project(model);
    let compact = area.width < COMPACT_WIDTH || area.height <= 8;
    let frame = FrameLayout::resolve(model, area);
    let transcript = frame.transcript;
    let transcript_area = if transcript.height >= 7 {
        transcript
    } else {
        area
    };
    let modal_area = transcript_area;
    let visible_items = projection.items.len().clamp(1, MAX_VISIBLE_ITEMS);
    // Top rule + search + item rows + action region. The visible range belongs
    // in the title instead of consuming a second metadata row.
    let chrome_rows = if compact { 4 } else { 3 };
    let popup_height = u16::try_from(visible_items)
        .unwrap_or(u16::MAX)
        .saturating_add(chrome_rows)
        .min(modal_area.height);
    let popup = Rect::new(
        modal_area.x,
        modal_area.bottom().saturating_sub(popup_height),
        modal_area.width,
        popup_height,
    );
    let inner = BottomPaneFrame::resolve(popup).inner();
    let first_item_row = inner.y.saturating_add(1);
    let action_rows = if compact { 2 } else { 1 };
    let action_row = inner
        .bottom()
        .saturating_sub(action_rows)
        .max(first_item_row.saturating_add(1));
    let capacity = usize::from(action_row.saturating_sub(first_item_row)).max(1);
    let window = selection_window(projection.items.len(), projection.selected, capacity);
    PaletteLayout {
        popup,
        inner,
        window,
        first_item_row,
        action_row,
        compact,
    }
}

pub(super) fn render(model: &AppModel, colors: Palette, area: Rect, buffer: &mut Buffer) {
    let projection = project(model);
    let layout = layout(model, area);
    BottomPaneFrame::resolve(layout.popup).render(
        Line::styled(palette_title(&projection, layout), colors.title),
        colors,
        buffer,
    );
    render_line(
        search_line(projection.query, colors),
        layout.inner,
        layout.inner.y,
        buffer,
    );
    if projection.items.is_empty() {
        render_line(
            Line::styled("  No matching commands", colors.muted),
            layout.inner,
            layout.first_item_row,
            buffer,
        );
    } else {
        for (offset, item) in projection.items[layout.window.0..layout.window.1]
            .iter()
            .enumerate()
        {
            let selection = layout.window.0 + offset;
            let row = layout
                .first_item_row
                .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
            render_line(
                item_line(
                    item,
                    projection.query,
                    selection == projection.selected,
                    layout,
                    colors,
                ),
                layout.inner,
                row,
                buffer,
            );
        }
    }

    if !layout.compact {
        render_line(
            key_hints(
                &[
                    ("↑/↓", "select"),
                    ("Home/End", "edge"),
                    ("Enter", "run"),
                    ("Esc", "close"),
                ],
                colors,
            ),
            layout.inner,
            layout.action_row,
            buffer,
        );
    } else {
        render_line(
            key_hints(&[("↑/↓", "select"), ("Home/End", "edge")], colors),
            layout.inner,
            layout.action_row,
            buffer,
        );
        render_line(
            key_hints(&[("Enter", "run"), ("Esc", "close")], colors),
            layout.inner,
            layout.action_row.saturating_add(1),
            buffer,
        );
    }
}

pub(super) fn selection_at(model: &AppModel, area: Rect, column: u16, row: u16) -> Option<usize> {
    let projection = project(model);
    let layout = layout(model, area);
    let visible_count = layout.visible_count();
    if column < layout.inner.x
        || column >= layout.inner.right()
        || row < layout.first_item_row
        || row
            >= layout
                .first_item_row
                .saturating_add(u16::try_from(visible_count).ok()?)
    {
        return None;
    }
    let selection = layout.window.0 + usize::from(row - layout.first_item_row);
    projection.items.get(selection).map(|_| selection)
}

pub(super) fn contains(model: &AppModel, area: Rect, column: u16, row: u16) -> bool {
    layout(model, area).popup.contains((column, row).into())
}

pub(in crate::view) fn linear_text(model: &AppModel) -> String {
    let projection = project(model);
    let layout = layout(
        model,
        Rect::new(
            0,
            0,
            model.terminal_size.width.max(40),
            model.terminal_size.height.max(8),
        ),
    );
    let mut lines = vec![
        "Command palette.".to_owned(),
        format!(
            "Search: {}.",
            if projection.query.is_empty() {
                "empty"
            } else {
                projection.query
            }
        ),
        spoken_window(&projection, layout),
    ];
    lines.extend(
        projection.items[layout.window.0..layout.window.1]
            .iter()
            .enumerate()
            .map(|(offset, item)| {
                let index = layout.window.0 + offset;
                let selected = if index == projection.selected {
                    "Selected"
                } else {
                    "Command"
                };
                format!(
                    "{selected} {} of {}: {}. {}.{}",
                    index + 1,
                    projection.items.len(),
                    item.input,
                    item.help,
                    item.unavailable_reason
                        .map(|reason| format!(" Unavailable: {reason}."))
                        .unwrap_or_default()
                )
            }),
    );
    if projection.items.is_empty() {
        lines.push("No matching commands.".into());
    }
    lines.push(
        "Use Up and Down to select, Home and End for edges, Enter to run, or Escape to close."
            .into(),
    );
    lines.join(" ")
}

fn item_line(
    item: &PaletteItem,
    query: &str,
    selected: bool,
    layout: PaletteLayout,
    colors: Palette,
) -> Line<'static> {
    let width = usize::from(layout.inner.width);
    let input_width = if layout.compact { 16 } else { 18 };
    let detail_width = width.saturating_sub(input_width + 4);
    let detail_style = if item.unavailable_reason.is_some() {
        colors.warning
    } else {
        colors.normal
    };
    let mut spans = vec![SelectionRow::marker_only(selected).marker(colors)];
    spans.extend(highlighted_field(
        item.input,
        query,
        input_width,
        colors.normal,
    ));
    spans.push(Span::styled("  ", colors.normal));
    spans.extend(highlighted_field(
        &item.detail,
        query,
        detail_width,
        detail_style,
    ));
    Line::from(spans)
}

fn search_line(query: &str, colors: Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled("Search  ", colors.title),
        Span::styled(
            if query.is_empty() {
                "type to search".into()
            } else {
                safe_text(query)
            },
            if query.is_empty() {
                colors.placeholder
            } else {
                colors.normal
            },
        ),
    ])
}

fn palette_title(projection: &PaletteProjection<'_>, layout: PaletteLayout) -> String {
    let total = projection.items.len();
    if total == 0 {
        return " Command palette ".into();
    }
    let (start, end) = layout.window;
    format!(" Command palette · {}–{end}/{total} ", start + 1)
}

fn spoken_window(projection: &PaletteProjection<'_>, layout: PaletteLayout) -> String {
    let total = projection.items.len();
    if total == 0 {
        return "0 matching commands.".into();
    }
    let (start, end) = layout.window;
    format!(
        "Showing commands {} through {end} of {total}; {} earlier and {} later.",
        start + 1,
        start,
        total - end
    )
}

fn render_line(line: Line<'static>, inner: Rect, row: u16, buffer: &mut Buffer) {
    if row < inner.bottom() {
        Paragraph::new(line).render(Rect::new(inner.x, row, inner.width, 1), buffer);
    }
}

#[cfg(test)]
mod tests;
