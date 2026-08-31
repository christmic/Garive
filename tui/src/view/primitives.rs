use ratatui::{
    layout::Rect,
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::style::Palette;

pub(super) fn key_hints(items: &[(&str, &str)], colors: Palette) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, label)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", colors.muted));
        }
        spans.push(Span::styled(format!(" {key} "), colors.keycap));
        spans.push(Span::styled((*label).to_owned(), colors.muted));
    }
    Line::from(spans)
}

pub(super) fn centered_popup(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub(super) fn centered_column(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y,
        width,
        area.height,
    )
}

pub(super) fn selection_window(total: usize, selected: usize, capacity: usize) -> (usize, usize) {
    if total == 0 || capacity == 0 {
        return (0, 0);
    }
    let selected = selected.min(total - 1);
    let start = selected
        .saturating_add(1)
        .saturating_sub(capacity)
        .min(total.saturating_sub(capacity));
    (start, (start + capacity).min(total))
}

pub(super) fn truncate_display(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(value) <= width {
        return value.to_owned();
    }
    let content_width = width.saturating_sub(1);
    let mut used = 0;
    let mut result = String::new();
    for grapheme in value.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used + grapheme_width > content_width {
            break;
        }
        result.push_str(grapheme);
        used += grapheme_width;
    }
    result.push('…');
    result
}
