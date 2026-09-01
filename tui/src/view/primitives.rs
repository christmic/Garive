use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{style::Palette, MotionFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RoleMarker {
    User,
    Agent,
}

impl RoleMarker {
    pub(super) fn span(self, colors: Palette) -> Span<'static> {
        match self {
            Self::User => Span::styled("› ", colors.request_marker),
            Self::Agent => Span::styled("• ", colors.agent),
        }
    }

    pub(super) fn prepend_to(self, line: &mut Line<'static>, colors: Palette) {
        let self_marking_heading =
            self == Self::Agent && line.to_string().trim_start().starts_with('#');
        if !self_marking_heading {
            line.spans.insert(0, self.span(colors));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LiveCaret {
    visible: bool,
}

impl LiveCaret {
    pub(super) const fn for_output(available: bool, ended: bool, motion: MotionFrame) -> Self {
        Self {
            visible: available && !ended && motion.live_caret_visible(),
        }
    }

    pub(super) fn append_to(self, line: &mut Line<'static>, colors: Palette) {
        if self.visible {
            line.spans.push(Span::styled("▍", colors.accent));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectionExtent {
    MarkerOnly,
    FullArea,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SelectionRow {
    selected: bool,
    extent: SelectionExtent,
}

impl SelectionRow {
    pub(super) const fn marker_only(selected: bool) -> Self {
        Self {
            selected,
            extent: SelectionExtent::MarkerOnly,
        }
    }

    pub(super) const fn full_area(selected: bool) -> Self {
        Self {
            selected,
            extent: SelectionExtent::FullArea,
        }
    }

    pub(super) fn marker(self, colors: Palette) -> Span<'static> {
        Span::styled(
            if self.selected { "› " } else { "  " },
            if self.selected {
                colors.selection_row.patch(colors.selected)
            } else {
                colors.muted
            },
        )
    }

    pub(super) fn paint(self, area: Rect, colors: Palette, buffer: &mut Buffer) {
        if self.selected && self.extent == SelectionExtent::FullArea {
            buffer.set_style(area, colors.selection_row);
        }
    }

    pub(super) fn style(self, colors: Palette, fallback: Style) -> Style {
        if self.selected && self.extent == SelectionExtent::FullArea {
            colors.selection_row
        } else {
            fallback
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ModalFrame {
    popup: Rect,
    inner: Rect,
}

/// Composer-aligned selection surface with one quiet top boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BottomPaneFrame {
    pane: Rect,
    inner: Rect,
}

impl BottomPaneFrame {
    pub(super) fn resolve(pane: Rect) -> Self {
        let inner = Block::default().borders(Borders::TOP).inner(pane);
        Self { pane, inner }
    }

    pub(super) const fn inner(self) -> Rect {
        self.inner
    }

    pub(super) fn render(self, title: Line<'static>, colors: Palette, buffer: &mut Buffer) {
        Clear.render(self.pane, buffer);
        buffer.set_style(self.pane, colors.request_surface);
        Block::default()
            .title(title)
            .borders(Borders::TOP)
            .border_set(colors.border_set())
            .border_style(colors.border)
            .render(self.pane, buffer);
    }
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
        let quiet_band = modal_quiet_band(self.popup, viewport);
        Clear.render(quiet_band, buffer);
        buffer.set_style(quiet_band, colors.modal_backdrop);
        Clear.render(self.popup, buffer);
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_set(colors.border_set())
            .border_style(colors.overlay_border)
            .padding(Padding::ZERO)
            .render(self.popup, buffer);
    }
}

fn modal_quiet_band(popup: Rect, viewport: Rect) -> Rect {
    let y = popup.y.max(viewport.y);
    let bottom = popup.bottom().min(viewport.bottom());
    Rect::new(viewport.x, y, viewport.width, bottom.saturating_sub(y))
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
    fn modal_frame_owns_padding_and_same_height_quiet_band() {
        let frame = ModalFrame::resolve(Rect::new(4, 2, 12, 6), Padding::new(2, 2, 1, 1));
        assert_eq!(frame.inner(), Rect::new(7, 4, 6, 2));
        assert_eq!(
            modal_quiet_band(Rect::new(4, 2, 12, 6), Rect::new(3, 1, 14, 8)),
            Rect::new(3, 2, 14, 6)
        );
    }

    #[test]
    fn modal_frame_clears_only_the_full_width_rows_it_occupies() {
        let viewport = Rect::new(0, 0, 20, 10);
        let popup = Rect::new(4, 2, 12, 6);
        let mut buffer = Buffer::empty(viewport);
        for y in viewport.top()..viewport.bottom() {
            for x in viewport.left()..viewport.right() {
                buffer[(x, y)].set_symbol("x");
            }
        }

        ModalFrame::resolve(popup, Padding::ZERO).render(
            viewport,
            Line::raw(" Modal "),
            super::super::palette(crate::Theme::Mono),
            &mut buffer,
        );

        for y in popup.top()..popup.bottom() {
            assert!((viewport.left()..viewport.right()).all(|x| buffer[(x, y)].symbol() != "x"));
        }
        for y in [popup.top() - 1, popup.bottom()] {
            assert!((viewport.left()..viewport.right()).all(|x| buffer[(x, y)].symbol() == "x"));
        }
    }

    #[test]
    fn bottom_pane_uses_one_top_rule_and_keeps_the_composer_axis() {
        let pane = Rect::new(0, 1, 20, 5);
        let frame = BottomPaneFrame::resolve(pane);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 6));
        let colors = super::super::palette(crate::Theme::Mono);

        frame.render(Line::raw(" Commands "), colors, &mut buffer);

        assert_eq!(frame.inner(), Rect::new(0, 2, 20, 4));
        let rule = colors.border_set().horizontal_top;
        assert!((0..20).any(|column| buffer[(column, 1)].symbol() == rule));
        assert!((2..6).all(|row| buffer[(0, row)].symbol() != "│"));
        assert!((2..6).all(|row| buffer[(19, row)].symbol() != "│"));
    }

    #[test]
    fn selection_extent_distinguishes_marker_from_area_emphasis() {
        let colors = super::super::palette(crate::Theme::Mono);
        let area = Rect::new(1, 1, 4, 2);
        let mut marker_buffer = Buffer::empty(Rect::new(0, 0, 8, 4));
        SelectionRow::marker_only(true).paint(area, colors, &mut marker_buffer);
        assert!(!marker_buffer[(2, 1)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED));

        let mut area_buffer = Buffer::empty(Rect::new(0, 0, 8, 4));
        SelectionRow::full_area(true).paint(area, colors, &mut area_buffer);
        assert!(area_buffer[(2, 2)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED));
    }

    #[test]
    fn role_markers_and_live_caret_preserve_non_color_identity() {
        let colors = super::super::palette(crate::Theme::Dark);
        assert_eq!(RoleMarker::User.span(colors).content, "› ");
        assert_eq!(RoleMarker::Agent.span(colors).content, "• ");

        let mut heading = Line::raw("## Release plan");
        RoleMarker::Agent.prepend_to(&mut heading, colors);
        assert_eq!(heading.to_string(), "## Release plan");

        let mut prose = Line::raw("Release is ready.");
        RoleMarker::Agent.prepend_to(&mut prose, colors);
        assert_eq!(prose.to_string(), "• Release is ready.");

        let mut active = Line::raw("answer");
        LiveCaret::for_output(true, false, MotionFrame::animated(0)).append_to(&mut active, colors);
        assert_eq!(active.to_string(), "answer▍");
        let mut quiet = Line::raw("answer");
        LiveCaret::for_output(true, false, MotionFrame::animated(4)).append_to(&mut quiet, colors);
        assert_eq!(quiet.to_string(), "answer");
        let mut reduced = Line::raw("answer");
        LiveCaret::for_output(true, false, MotionFrame::reduced()).append_to(&mut reduced, colors);
        assert_eq!(reduced.to_string(), "answer");
    }
}
