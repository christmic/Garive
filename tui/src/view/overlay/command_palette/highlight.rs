use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::super::super::safe_text;

pub(super) fn highlighted_field(
    value: &str,
    query: &str,
    width: usize,
    base: Style,
) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let safe = safe_text(value);
    let graphemes = safe.graphemes(true).collect::<Vec<_>>();
    let matches = matching_graphemes(&graphemes, query);
    let full_width = UnicodeWidthStr::width(safe.as_str());
    let truncated = full_width > width;
    let budget = width.saturating_sub(usize::from(truncated));
    let mut spans = Vec::new();
    let mut used = 0usize;
    for (index, grapheme) in graphemes.iter().enumerate() {
        let grapheme_width = UnicodeWidthStr::width(*grapheme);
        if used.saturating_add(grapheme_width) > budget {
            break;
        }
        let style = if matches[index] {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        };
        spans.push(Span::styled((*grapheme).to_owned(), style));
        used = used.saturating_add(grapheme_width);
    }
    if truncated {
        spans.push(Span::styled("…", base));
        used = used.saturating_add(1);
    }
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), base));
    }
    spans
}

fn matching_graphemes(graphemes: &[&str], query: &str) -> Vec<bool> {
    let mut mask = vec![false; graphemes.len()];
    let mut lowered = String::new();
    let mut ranges = Vec::with_capacity(graphemes.len());
    for grapheme in graphemes {
        let start = lowered.len();
        lowered.push_str(&grapheme.to_lowercase());
        ranges.push((start, lowered.len()));
    }
    for term in safe_text(query)
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| !term.is_empty())
    {
        for (start, _) in lowered.match_indices(&term) {
            let end = start.saturating_add(term.len());
            for (index, (grapheme_start, grapheme_end)) in ranges.iter().enumerate() {
                if *grapheme_start < end && *grapheme_end > start {
                    mask[index] = true;
                }
            }
        }
    }
    mask
}
