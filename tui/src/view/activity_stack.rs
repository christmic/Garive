//! Compact presentation for one contiguous run of public activity cells.

use ratatui::text::{Line, Span};

use crate::{
    application::{TimelineItem, TimelineTone},
    Theme,
};
use unicode_width::UnicodeWidthStr;

use super::{palette, primitives::truncate_display, safe_text};

pub(super) fn render(items: &[TimelineItem], theme: Theme, width: u16) -> Vec<Line<'static>> {
    debug_assert!(items
        .iter()
        .all(|item| item.role == crate::application::TimelineRole::Status));
    let colors = palette(theme);
    let completed_items = items
        .iter()
        .filter(|item| item.tone == TimelineTone::Success)
        .collect::<Vec<_>>();
    let completed = completed_items.len();
    let primary = items.iter().rev().find(|item| {
        matches!(
            item.tone,
            TimelineTone::Active
                | TimelineTone::Warning
                | TimelineTone::Danger
                | TimelineTone::Neutral
        )
    });
    let completed_copy = completed_items.last().map(|item| safe_text(&item.text));

    if width < 52 {
        let available = usize::from(width.saturating_sub(2));
        let compact = match (primary, completed_copy.as_deref()) {
            (Some(item), _) if completed > 0 => {
                let suffix = format!(" · ✓{completed}");
                let leading = format!("{} {}", icon(item.tone), safe_text(&item.text));
                format!(
                    "{}{}",
                    truncate_display(
                        &leading,
                        available.saturating_sub(UnicodeWidthStr::width(suffix.as_str()))
                    ),
                    suffix
                )
            }
            (Some(item), _) => format!("{} {}", icon(item.tone), safe_text(&item.text)),
            (None, Some(done)) => {
                let additional = completed.saturating_sub(1);
                if additional == 0 {
                    format!("✓ {done}")
                } else {
                    let suffix = format!(" · +{additional}");
                    let leading = format!("✓ {done}");
                    format!(
                        "{}{}",
                        truncate_display(
                            &leading,
                            available.saturating_sub(UnicodeWidthStr::width(suffix.as_str()))
                        ),
                        suffix
                    )
                }
            }
            (None, None) => return Vec::new(),
        };
        return vec![Line::styled(
            format!("  {}", truncate_display(&compact, available)),
            primary.map_or(colors.muted, |item| tone_style(item.tone, colors)),
        )];
    }

    let mut lines = Vec::with_capacity(3);
    let group_tone = primary.map_or(TimelineTone::Success, |item| item.tone);
    lines.push(group_header(group_tone, colors));
    if let Some(done) = completed_copy {
        lines.push(completed_line(
            done,
            completed.saturating_sub(1),
            primary.is_some(),
            colors,
            width,
        ));
    }
    if let Some(item) = primary {
        lines.push(detail_line(
            "└",
            safe_text(&item.text),
            String::new(),
            colors.normal,
            colors,
            width,
        ));
    }
    lines
}

fn group_header(tone: TimelineTone, colors: super::style::Palette) -> Line<'static> {
    let heading = match tone {
        TimelineTone::Active => "Working",
        TimelineTone::Success => "Completed",
        TimelineTone::Warning => "Attention",
        TimelineTone::Danger => "Failed",
        TimelineTone::Neutral => "Activity",
    };
    Line::from(vec![
        Span::styled("• ", tone_style(tone, colors)),
        Span::styled(heading, colors.title),
    ])
}

fn completed_line(
    text: String,
    additional: usize,
    has_primary: bool,
    colors: super::style::Palette,
    width: u16,
) -> Line<'static> {
    let suffix = if additional > 0 {
        format!(" · +{additional} earlier")
    } else {
        String::new()
    };
    detail_line(
        if has_primary { "├" } else { "└" },
        text,
        suffix,
        colors.muted,
        colors,
        width,
    )
}

fn detail_line(
    branch: &'static str,
    text: String,
    suffix: String,
    text_style: ratatui::style::Style,
    colors: super::style::Palette,
    width: u16,
) -> Line<'static> {
    let available = usize::from(width.saturating_sub(4));
    let suffix_width = UnicodeWidthStr::width(suffix.as_str());
    let text = truncate_display(&text, available.saturating_sub(suffix_width));
    Line::from(vec![
        Span::styled(format!("  {branch} "), colors.muted),
        Span::styled(text, text_style),
        Span::styled(suffix, colors.muted),
    ])
}

fn icon(tone: TimelineTone) -> &'static str {
    match tone {
        TimelineTone::Active => "●",
        TimelineTone::Success => "✓",
        TimelineTone::Warning => "!",
        TimelineTone::Danger => "×",
        TimelineTone::Neutral => "○",
    }
}

fn tone_style(tone: TimelineTone, colors: super::style::Palette) -> ratatui::style::Style {
    match tone {
        TimelineTone::Active => colors.accent,
        TimelineTone::Success => colors.success,
        TimelineTone::Warning => colors.warning,
        TimelineTone::Danger => colors.danger,
        TimelineTone::Neutral => colors.activity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::TimelineRole;

    fn item(key: &str, tone: TimelineTone, text: &str) -> TimelineItem {
        TimelineItem {
            stable_key: key.into(),
            position: 1,
            role: TimelineRole::Status,
            tone,
            text: text.into(),
        }
    }

    #[test]
    fn collapses_completed_siblings_but_preserves_the_current_safe_activity() {
        let items = vec![
            item("a", TimelineTone::Success, "Read config"),
            item("b", TimelineTone::Success, "Checked tests"),
            item("c", TimelineTone::Active, "Running cargo test"),
        ];
        let standard = render(&items, Theme::Mono, 80);
        assert_eq!(standard.len(), 3);
        assert_eq!(standard[0].to_string(), "• Working");
        assert_eq!(standard[1].to_string(), "  ├ Checked tests · +1 earlier");
        assert_eq!(standard[2].to_string(), "  └ Running cargo test");
        let compact = render(&items, Theme::Mono, 40);
        assert_eq!(compact.len(), 1);
        assert_eq!(compact[0].to_string(), "  ● Running cargo test · ✓2");
    }

    #[test]
    fn compact_counter_survives_wide_grapheme_truncation() {
        let items = vec![
            item("a", TimelineTone::Success, "Read config"),
            item("b", TimelineTone::Success, "Checked tests"),
            item(
                "c",
                TimelineTone::Active,
                "读取界面 🦀 and validate the streamed response",
            ),
        ];

        let line = render(&items, Theme::Mono, 40)[0].to_string();
        assert!(line.ends_with(" · ✓2"));
        assert!(UnicodeWidthStr::width(line.as_str()) <= 40);
    }

    #[test]
    fn compact_completed_stack_keeps_the_latest_label_and_sibling_count() {
        let items = vec![
            item("a", TimelineTone::Success, "Read config"),
            item(
                "b",
                TimelineTone::Success,
                "检查界面 🦀 and verify the final output",
            ),
        ];

        let line = render(&items, Theme::Mono, 24)[0].to_string();
        assert!(line.ends_with(" · +1"));
        assert!(line.contains("检查界面"));
        assert!(UnicodeWidthStr::width(line.as_str()) <= 24);
    }

    #[test]
    fn completed_group_uses_one_lifecycle_heading_and_one_detail_marker() {
        let items = vec![item("a", TimelineTone::Success, "Read file")];

        let lines = render(&items, Theme::Mono, 80);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "• Completed");
        assert_eq!(lines[1].to_string(), "  └ Read file");
    }
}
