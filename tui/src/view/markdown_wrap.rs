//! Style-preserving physical reflow for Markdown prose and list items.

use ratatui::text::Span;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) fn wrap_styled(
    spans: &[Span<'static>],
    first_width: usize,
    hanging: usize,
) -> Vec<Vec<Span<'static>>> {
    let graphemes = spans
        .iter()
        .flat_map(|span| {
            span.content
                .graphemes(true)
                .map(|value| (value.to_owned(), span.style))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if graphemes.is_empty() {
        return vec![Vec::new()];
    }
    let mut rows = Vec::new();
    let mut start = 0;
    while start < graphemes.len() {
        let capacity = if rows.is_empty() {
            first_width
        } else {
            first_width.saturating_sub(hanging).max(1)
        };
        let mut used = 0_usize;
        let mut end = start;
        let mut last_space = None;
        while end < graphemes.len() {
            let width = UnicodeWidthStr::width(graphemes[end].0.as_str());
            if end > start && used.saturating_add(width) > capacity {
                break;
            }
            used = used.saturating_add(width);
            end += 1;
            if graphemes[end - 1].0.chars().all(char::is_whitespace) {
                last_space = Some(end);
            }
            if used >= capacity {
                break;
            }
        }
        let cut = if end < graphemes.len() {
            last_space
                .filter(|cut| *cut > start)
                .unwrap_or(end.max(start + 1))
        } else {
            end
        };
        let visible_end = (start..cut)
            .rev()
            .find(|index| !graphemes[*index].0.chars().all(char::is_whitespace))
            .map_or(start, |index| index + 1);
        let mut row: Vec<Span<'static>> = Vec::new();
        for (value, style) in &graphemes[start..visible_end] {
            if let Some(last) = row.last_mut().filter(|last| last.style == *style) {
                last.content.to_mut().push_str(value);
            } else {
                row.push(Span::styled(value.clone(), *style));
            }
        }
        rows.push(row);
        start = cut;
        while start < graphemes.len() && graphemes[start].0.chars().all(char::is_whitespace) {
            start += 1;
        }
    }
    rows
}
