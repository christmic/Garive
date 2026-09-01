use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::style::Palette;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FocusFrameTone {
    Neutral,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ModalFrame {
    popup: Rect,
    inner: Rect,
}

impl ModalFrame {
    pub(super) fn resolve(popup: Rect, padding: Padding) -> Self {
        let inner = Block::default()
            .borders(Borders::ALL)
            .padding(padding)
            .inner(popup);
        Self { popup, inner }
    }

    pub(super) const fn inner(self) -> Rect {
        self.inner
    }

    pub(super) fn render(
        self,
        viewport: Rect,
        title: Line<'static>,
        colors: Palette,
        buffer: &mut Buffer,
    ) {
        buffer.set_style(viewport, colors.modal_backdrop);
        let halo = modal_halo(self.popup, viewport);
        Clear.render(halo, buffer);
        buffer.set_style(halo, colors.modal_backdrop);
        Clear.render(self.popup, buffer);
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(colors.overlay_border)
            .padding(Padding::ZERO)
            .render(self.popup, buffer);
    }
}

fn modal_halo(popup: Rect, viewport: Rect) -> Rect {
    let x = popup.x.saturating_sub(2).max(viewport.x);
    let right = popup.right().saturating_add(2).min(viewport.right());
    Rect::new(
        x,
        popup.y.max(viewport.y),
        right.saturating_sub(x),
        popup
            .bottom()
            .min(viewport.bottom())
            .saturating_sub(popup.y),
    )
}

pub(super) fn focus_frame(
    colors: Palette,
    tone: FocusFrameTone,
    marker: Option<Line<'static>>,
) -> Block<'static> {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(match tone {
            FocusFrameTone::Neutral => colors.border,
            FocusFrameTone::Warning => colors.warning,
        })
        .padding(Padding::horizontal(1));
    match marker {
        Some(marker) => block.title(marker),
        None => block,
    }
}

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

pub(super) fn selection_marker(selected: bool, colors: Palette) -> Span<'static> {
    Span::styled(
        if selected { "› " } else { "  " },
        if selected {
            colors.selection_row.patch(colors.selected)
        } else {
            colors.muted
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_frame_owns_padding_and_bounded_halo_geometry() {
        let frame = ModalFrame::resolve(Rect::new(4, 2, 12, 6), Padding::new(2, 2, 1, 1));
        assert_eq!(frame.inner(), Rect::new(7, 4, 6, 2));
        assert_eq!(
            modal_halo(Rect::new(4, 2, 12, 6), Rect::new(3, 1, 14, 8)),
            Rect::new(3, 2, 14, 6)
        );
    }
}
