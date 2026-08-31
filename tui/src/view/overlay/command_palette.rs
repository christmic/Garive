use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget},
};

use crate::{application::AppModel, input::COMMAND_PALETTE};

use super::super::{
    layout::FrameLayout,
    primitives::{centered_popup, key_hints, selection_window, truncate_display},
    safe_text,
    style::Palette,
};

const DESIRED_WIDTH: u16 = 74;
const DESIRED_HEIGHT: u16 = 21;
const COMPACT_WIDTH: u16 = 50;

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
    let modal_area = if compact {
        area
    } else {
        let transcript = FrameLayout::resolve(model, area).transcript;
        if transcript.height >= 8 {
            transcript
        } else {
            area
        }
    };
    let gutter = if compact { 0 } else { 4 };
    let popup_width = DESIRED_WIDTH.min(modal_area.width.saturating_sub(gutter));
    let popup_height = DESIRED_HEIGHT.min(modal_area.height);
    let popup = centered_popup(modal_area, popup_width, popup_height);
    let inner = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::horizontal(2))
        .inner(popup);
    let first_item_row = inner.y.saturating_add(2);
    let action_rows = if compact { 2 } else { 1 };
    let spacer_rows = u16::from(!compact);
    let action_row = inner
        .bottom()
        .saturating_sub(action_rows)
        .saturating_sub(spacer_rows)
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
    buffer.set_style(area, colors.modal_backdrop);
    let halo = modal_halo(layout.popup, area);
    Clear.render(halo, buffer);
    buffer.set_style(halo, colors.modal_backdrop);
    Clear.render(layout.popup, buffer);
    Block::default()
        .title(Line::styled(" Command palette ", colors.title))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(colors.overlay_border)
        .padding(Padding::horizontal(2))
        .render(layout.popup, buffer);

    render_line(
        search_line(projection.query, colors),
        layout.inner,
        layout.inner.y,
        buffer,
    );
    render_line(
        window_line(&projection, layout, colors),
        layout.inner,
        layout.inner.y.saturating_add(1),
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
            if selection == projection.selected {
                buffer.set_style(
                    Rect::new(layout.inner.x, row, layout.inner.width, 1),
                    colors.selection_row,
                );
            }
            render_line(
                item_line(item, selection == projection.selected, layout, colors),
                layout.inner,
                row,
                buffer,
            );
        }
    }

    if !layout.compact {
        render_line(Line::default(), layout.inner, layout.action_row, buffer);
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
            layout.action_row.saturating_add(1),
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
    selected: bool,
    layout: PaletteLayout,
    colors: Palette,
) -> Line<'static> {
    let width = usize::from(layout.inner.width);
    let input_width = if layout.compact { 16 } else { 18 };
    let detail_width = width.saturating_sub(input_width + 4);
    Line::from(vec![
        Span::styled(if selected { "› " } else { "  " }, colors.selected),
        Span::styled(
            format!("{:<input_width$}  ", item.input),
            if selected {
                colors.selected
            } else {
                colors.normal
            },
        ),
        Span::styled(
            truncate_display(&safe_text(&item.detail), detail_width),
            if item.unavailable_reason.is_some() {
                colors.warning
            } else {
                colors.normal
            },
        ),
    ])
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

fn window_line(
    projection: &PaletteProjection<'_>,
    layout: PaletteLayout,
    colors: Palette,
) -> Line<'static> {
    Line::styled(
        truncate_display(
            &visual_window(projection, layout),
            usize::from(layout.inner.width),
        ),
        colors.muted,
    )
}

fn visual_window(projection: &PaletteProjection<'_>, layout: PaletteLayout) -> String {
    let total = projection.items.len();
    if total == 0 {
        return "0 commands · refine the search".into();
    }
    let (start, end) = layout.window;
    if layout.compact {
        let mut parts = vec![format!("Showing {}–{end} / {total}", start + 1)];
        if start > 0 {
            parts.push(format!("↑{start}"));
        }
        if end < total {
            parts.push(format!("↓{}", total - end));
        }
        return parts.join(" · ");
    }
    let mut parts = vec![format!("{total} commands · showing {}–{end}", start + 1)];
    if start > 0 {
        parts.push(format!("↑ {start} earlier"));
    }
    if end < total {
        parts.push(format!("↓ {} more", total - end));
    }
    parts.join(" · ")
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

fn modal_halo(popup: Rect, area: Rect) -> Rect {
    let x = popup.x.saturating_sub(2).max(area.x);
    let right = popup.right().saturating_add(2).min(area.right());
    Rect::new(
        x,
        popup.y.max(area.y),
        right.saturating_sub(x),
        popup.bottom().min(area.bottom()).saturating_sub(popup.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{Overlay, TerminalSize};

    fn model(width: u16, height: u16, selection: usize) -> AppModel {
        AppModel {
            overlay: Some(Overlay::CommandPalette),
            terminal_size: TerminalSize { width, height },
            command_selection: selection,
            ..Default::default()
        }
    }

    #[test]
    fn compact_layout_always_reserves_real_command_rows_and_safe_actions() {
        let model = model(40, 8, COMMAND_PALETTE.len() - 1);
        let layout = layout(&model, Rect::new(0, 0, 40, 8));
        assert!(layout.item_capacity() >= 1);
        assert_eq!(layout.window.1, COMMAND_PALETTE.len());
        assert!(layout.first_item_row < layout.action_row);
        assert!(layout.action_row.saturating_add(1) < layout.inner.bottom());
    }

    #[test]
    fn every_command_can_own_a_visible_window() {
        for selected in 0..COMMAND_PALETTE.len() {
            let model = model(160, 28, selected);
            let layout = layout(&model, Rect::new(0, 0, 160, 28));
            assert!(layout.window.0 <= selected);
            assert!(selected < layout.window.1);
        }
    }

    #[test]
    fn hit_testing_only_maps_rendered_item_rows() {
        let model = model(40, 8, COMMAND_PALETTE.len() - 1);
        let area = Rect::new(0, 0, 40, 8);
        let layout = layout(&model, area);
        assert_eq!(
            selection_at(&model, area, layout.inner.x, layout.first_item_row),
            Some(layout.window.0)
        );
        assert_eq!(
            selection_at(&model, area, layout.inner.x, layout.inner.y),
            None
        );
        assert_eq!(
            selection_at(&model, area, layout.inner.x, layout.action_row),
            None
        );
        assert_eq!(
            selection_at(&model, area, layout.inner.right(), layout.first_item_row),
            None
        );
    }

    #[test]
    fn linear_projection_announces_window_and_selected_absolute_index() {
        let model = model(40, 8, COMMAND_PALETTE.len() - 1);
        let spoken = linear_text(&model);
        assert!(spoken.contains("Showing commands"));
        assert!(spoken.contains("Selected 21 of 21: /quit"));
        assert!(spoken.contains("Home and End for edges"));
    }
}
