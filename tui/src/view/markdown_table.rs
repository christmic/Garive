use pulldown_cmark::Alignment;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const MAX_COLUMNS: usize = 12;
const MAX_ROWS: usize = 64;
const MAX_CELL_CHARS: usize = 4_096;

#[derive(Default)]
pub(super) struct TableBuilder {
    alignments: Vec<Alignment>,
    header: Vec<Vec<Span<'static>>>,
    rows: Vec<Vec<Vec<Span<'static>>>>,
    row: Vec<Vec<Span<'static>>>,
    cell: Option<Vec<Span<'static>>>,
    cell_chars: usize,
    heading: bool,
}

impl TableBuilder {
    pub(super) fn new(mut alignments: Vec<Alignment>) -> Self {
        alignments.truncate(MAX_COLUMNS);
        Self {
            alignments,
            ..Self::default()
        }
    }

    pub(super) fn start_row(&mut self, heading: bool) {
        self.finish_row();
        self.heading = heading;
    }

    pub(super) fn start_cell(&mut self) {
        self.finish_cell();
        self.cell_chars = 0;
        self.cell = (self.row.len() < MAX_COLUMNS).then(Vec::new);
    }

    pub(super) fn push(&mut self, span: Span<'static>) -> bool {
        let Some(cell) = self.cell.as_mut() else {
            return false;
        };
        let remaining = MAX_CELL_CHARS.saturating_sub(self.cell_chars);
        if remaining == 0 {
            return true;
        }
        let chars = span.content.chars().count();
        if chars <= remaining {
            self.cell_chars += chars;
            cell.push(span);
        } else {
            let mut value = span
                .content
                .chars()
                .take(remaining.saturating_sub(1))
                .collect::<String>();
            value.push('…');
            self.cell_chars = MAX_CELL_CHARS;
            cell.push(Span::styled(value, span.style));
        }
        true
    }

    pub(super) fn soft_break(&mut self, style: Style) -> bool {
        self.push(Span::styled(" ", style))
    }

    pub(super) fn finish_cell(&mut self) {
        if let Some(cell) = self.cell.take() {
            self.row.push(cell);
        }
    }

    pub(super) fn finish_row(&mut self) {
        self.finish_cell();
        if self.row.is_empty() {
            return;
        }
        let row = std::mem::take(&mut self.row);
        if self.heading && self.header.is_empty() {
            self.header = row;
        } else if self.rows.len() < MAX_ROWS {
            self.rows.push(row);
        }
    }

    pub(super) fn render(
        mut self,
        prefix: &str,
        quote_depth: usize,
        normal: Style,
        accent: Style,
        muted: Style,
        width: usize,
    ) -> Vec<Line<'static>> {
        self.finish_row();
        let columns = self
            .alignments
            .len()
            .max(self.header.len())
            .max(self.rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns == 0 {
            return Vec::new();
        }
        self.alignments.resize(columns, Alignment::None);
        normalize(&mut self.header, columns);
        for row in &mut self.rows {
            normalize(row, columns);
        }
        let lead_width = UnicodeWidthStr::width(prefix) + quote_depth.saturating_mul(2);
        let available = width.saturating_sub(lead_width);
        if self.rows.is_empty()
            || available >= columns.saturating_mul(6) + columns.saturating_sub(1) * 3
        {
            self.render_grid(prefix, quote_depth, normal, accent, muted, available)
        } else {
            self.render_records(prefix, quote_depth, accent, muted, available)
        }
    }

    fn render_grid(
        &self,
        prefix: &str,
        quote_depth: usize,
        normal: Style,
        accent: Style,
        muted: Style,
        available: usize,
    ) -> Vec<Line<'static>> {
        let columns = self.alignments.len();
        let content_width = available.saturating_sub((columns - 1) * 3);
        let widths = self.column_widths(content_width);
        let mut lines = Vec::new();
        if !self.header.is_empty() {
            append_grid_row(
                &mut lines,
                &self.header,
                &self.alignments,
                &widths,
                prefix,
                quote_depth,
                normal.add_modifier(Modifier::BOLD),
                muted,
            );
            let rule = widths
                .iter()
                .map(|width| "─".repeat(*width))
                .collect::<Vec<_>>()
                .join("─┼─");
            lines.push(with_lead(
                prefix,
                quote_depth,
                muted,
                vec![Span::styled(rule, accent)],
            ));
        }
        for row in &self.rows {
            append_grid_row(
                &mut lines,
                row,
                &self.alignments,
                &widths,
                prefix,
                quote_depth,
                normal,
                muted,
            );
        }
        lines
    }

    fn column_widths(&self, available: usize) -> Vec<usize> {
        let columns = self.alignments.len();
        let mut desired = vec![1; columns];
        for row in std::iter::once(&self.header).chain(&self.rows) {
            for (index, cell) in row.iter().enumerate() {
                desired[index] = desired[index].max(UnicodeWidthStr::width(plain(cell).as_str()));
            }
        }
        let minimum = 4.min(available / columns);
        let fair = available / columns;
        let mut widths = desired
            .iter()
            .map(|desired| (*desired).max(minimum).min(fair))
            .collect::<Vec<_>>();
        let mut remaining = available.saturating_sub(widths.iter().sum());
        while remaining > 0
            && widths
                .iter()
                .zip(&desired)
                .any(|(width, desired)| width < desired)
        {
            for index in 0..columns {
                if remaining == 0 {
                    break;
                }
                if widths[index] < desired[index] {
                    widths[index] += 1;
                    remaining -= 1;
                }
            }
        }
        widths
    }

    fn render_records(
        &self,
        prefix: &str,
        quote_depth: usize,
        accent: Style,
        muted: Style,
        available: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            if row_index > 0 {
                lines.push(with_lead(
                    prefix,
                    quote_depth,
                    muted,
                    vec![Span::styled("···", muted)],
                ));
            }
            for (index, cell) in row.iter().enumerate() {
                let fallback = format!("Column {}", index + 1);
                let label = self.header.get(index).filter(|cell| !cell.is_empty());
                let label = label.map_or(fallback, |cell| plain(cell));
                let label = truncate(&label, available.saturating_div(3).max(1));
                let label_width = UnicodeWidthStr::width(label.as_str()) + 2;
                let value_width = available.saturating_sub(label_width).max(1);
                let wrapped = wrap(cell, value_width);
                for (line_index, value) in wrapped.into_iter().enumerate() {
                    let mut spans = if line_index == 0 {
                        vec![
                            Span::styled(label.clone(), accent.add_modifier(Modifier::BOLD)),
                            Span::styled(": ", muted),
                        ]
                    } else {
                        vec![Span::raw(" ".repeat(label_width))]
                    };
                    spans.extend(value);
                    lines.push(with_lead(prefix, quote_depth, muted, spans));
                }
            }
        }
        lines
    }
}

fn normalize(row: &mut Vec<Vec<Span<'static>>>, columns: usize) {
    row.truncate(columns);
    row.resize_with(columns, Vec::new);
}

#[allow(clippy::too_many_arguments)]
fn append_grid_row(
    output: &mut Vec<Line<'static>>,
    row: &[Vec<Span<'static>>],
    alignments: &[Alignment],
    widths: &[usize],
    prefix: &str,
    quote_depth: usize,
    base: Style,
    muted: Style,
) {
    let cells = row
        .iter()
        .zip(widths)
        .map(|(cell, width)| wrap(cell, *width))
        .collect::<Vec<_>>();
    let height = cells.iter().map(Vec::len).max().unwrap_or(1);
    for line_index in 0..height {
        let mut spans = Vec::new();
        for index in 0..cells.len() {
            if index > 0 {
                spans.push(Span::styled(" │ ", muted));
            }
            let content = cells[index].get(line_index).cloned().unwrap_or_default();
            let used = content
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum();
            let padding = widths[index].saturating_sub(used);
            let (left, right) = match alignments[index] {
                Alignment::Right => (padding, 0),
                Alignment::Center => (padding / 2, padding - padding / 2),
                _ => (0, padding),
            };
            spans.push(Span::styled(" ".repeat(left), base));
            spans.extend(
                content
                    .into_iter()
                    .map(|span| Span::styled(span.content, base.patch(span.style))),
            );
            spans.push(Span::styled(" ".repeat(right), base));
        }
        output.push(with_lead(prefix, quote_depth, muted, spans));
    }
}

fn with_lead(
    prefix: &str,
    quote_depth: usize,
    muted: Style,
    mut content: Vec<Span<'static>>,
) -> Line<'static> {
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::raw(prefix.to_owned()));
    }
    if quote_depth > 0 {
        spans.push(Span::styled("│ ".repeat(quote_depth), muted));
    }
    spans.append(&mut content);
    Line::from(spans)
}

fn wrap(spans: &[Span<'static>], width: usize) -> Vec<Vec<Span<'static>>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut used = 0_usize;
    for span in spans {
        for grapheme in span.content.graphemes(true) {
            let size = UnicodeWidthStr::width(grapheme);
            if used > 0 && used.saturating_add(size) > width {
                lines.push(Vec::new());
                used = 0;
            }
            if size <= width {
                let line = lines.last_mut().unwrap();
                if let Some(last) = line.last_mut().filter(|last| last.style == span.style) {
                    last.content.to_mut().push_str(grapheme);
                } else {
                    line.push(Span::styled(grapheme.to_owned(), span.style));
                }
                used += size;
            }
        }
    }
    lines
}

fn plain(spans: &[Span<'static>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn truncate(value: &str, width: usize) -> String {
    value
        .graphemes(true)
        .scan(0, |used, part| {
            *used += UnicodeWidthStr::width(part);
            (*used <= width).then_some(part)
        })
        .collect()
}
