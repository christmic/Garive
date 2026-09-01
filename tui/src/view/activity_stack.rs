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

    let mut lines = Vec::with_capacity(2);
    if let Some(done) = completed_copy {
        lines.push(completed_line(done, completed.saturating_sub(1), colors));
    }
    if let Some(item) = primary {
        lines.push(activity_line(
            icon(item.tone),
            safe_text(&item.text),
            tone_style(item.tone, colors),
            colors.muted,
        ));
    }
    lines
}

fn completed_line(text: String, additional: usize, colors: super::style::Palette) -> Line<'static> {
    let mut spans = vec![
        Span::styled("  ✓  ", colors.success),
        Span::styled(text, colors.muted),
    ];
    if additional > 0 {
        spans.push(Span::styled(
            format!(" · +{additional} completed"),
            colors.muted.add_modifier(ratatui::style::Modifier::DIM),
        ));
    }
    Line::from(spans)
}

fn activity_line(
    icon: &'static str,
    text: String,
    icon_style: ratatui::style::Style,
    text_style: ratatui::style::Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {icon}  "), icon_style),
        Span::styled(text, text_style),
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
        assert_eq!(standard.len(), 2);
        assert_eq!(standard[0].to_string(), "  ✓  Checked tests · +1 completed");
        assert_eq!(standard[1].to_string(), "  ●  Running cargo test");
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
}
