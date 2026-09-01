use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::Theme;

use super::super::{palette, primitives::RoleMarker, safe_text};

const HANGING_INDENT: &str = "  ";

pub(super) fn render(source: &str, theme: Theme, width: u16) -> Vec<Line<'static>> {
    let colors = palette(theme);
    let width = usize::from(width.max(1));
    let content_width = width.saturating_sub(2).max(1);
    let safe = safe_text(source);
    let mut rows = Vec::new();
    for logical in safe.split('\n') {
        rows.extend(wrap_line(logical, content_width));
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    let mut lines = rows
        .into_iter()
        .enumerate()
        .map(|(index, content)| {
            let prefix = if index == 0 { "› " } else { HANGING_INDENT };
            let used = UnicodeWidthStr::width(prefix) + UnicodeWidthStr::width(content.as_str());
            let padding = " ".repeat(width.saturating_sub(used));
            Line::from(vec![
                if index == 0 {
                    RoleMarker::User.span(colors)
                } else {
                    Span::styled(HANGING_INDENT, colors.request_marker)
                },
                Span::styled(content, colors.request_surface),
                Span::styled(padding, colors.request_surface),
            ])
        })
        .collect::<Vec<_>>();
    if width >= 80 {
        let breathing_row = Line::from(Span::styled(" ".repeat(width), colors.request_surface));
        lines.insert(0, breathing_row.clone());
        lines.push(breathing_row);
    }
    lines
}

fn wrap_line(source: &str, width: usize) -> Vec<String> {
    let graphemes = source.graphemes(true).collect::<Vec<_>>();
    if graphemes.is_empty() {
        return vec![String::new()];
    }
    let mut rows = Vec::new();
    let mut start = 0;
    while start < graphemes.len() {
        let mut used = 0usize;
        let mut end = start;
        let mut last_space = None;
        while end < graphemes.len() {
            let grapheme_width = UnicodeWidthStr::width(graphemes[end]);
            if end > start && used.saturating_add(grapheme_width) > width {
                break;
            }
            used = used.saturating_add(grapheme_width);
            end += 1;
            if graphemes[end - 1].chars().all(char::is_whitespace) {
                last_space = Some(end);
            }
            if used >= width {
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
        rows.push(
            graphemes[start..cut]
                .concat()
                .trim_end_matches(char::is_whitespace)
                .to_owned(),
        );
        start = cut;
        while start < graphemes.len() && graphemes[start].chars().all(char::is_whitespace) {
            start += 1;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_is_display_width_bounded_and_preserves_graphemes() {
        assert_eq!(wrap_line("你好世界 release", 8), ["你好世界", "release"]);
        assert_eq!(
            wrap_line("e\u{301}e\u{301}e\u{301}", 2),
            ["e\u{301}e\u{301}", "e\u{301}"]
        );
    }

    #[test]
    fn standard_request_surface_breathes_while_compact_stays_tight() {
        let standard = render("Ship the verified release", Theme::Dark, 80);
        assert_eq!(standard.len(), 3);
        assert_eq!(standard[0].to_string(), " ".repeat(80));
        assert_eq!(
            standard[1].to_string(),
            format!("› Ship the verified release{}", " ".repeat(53))
        );
        assert_eq!(standard[2].to_string(), " ".repeat(80));
        assert!(standard
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.style.bg == palette(Theme::Dark).request_surface.bg));

        let compact = render("Ship the verified release", Theme::Dark, 79);
        assert_eq!(compact.len(), 1);
        assert!(compact[0].to_string().starts_with("› Ship"));
    }
}
