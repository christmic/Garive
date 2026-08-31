//! Compact presentation for one contiguous run of public activity cells.

use ratatui::text::{Line, Span};

use crate::{
    application::{TimelineItem, TimelineTone},
    Theme,
};

use super::{palette, primitives::truncate_display, safe_text};

pub(super) fn render(items: &[TimelineItem], theme: Theme, width: u16) -> Vec<Line<'static>> {
    debug_assert!(items
        .iter()
        .all(|item| item.role == crate::application::TimelineRole::Status));
    let colors = palette(theme);
    let completed = items
        .iter()
        .filter(|item| item.tone == TimelineTone::Success)
        .count();
    let primary = items.iter().rev().find(|item| {
        matches!(
            item.tone,
            TimelineTone::Active
                | TimelineTone::Warning
                | TimelineTone::Danger
                | TimelineTone::Neutral
        )
    });
    let completed_copy = match completed {
        0 => None,
        1 if primary.is_none() => items
            .iter()
            .find(|item| item.tone == TimelineTone::Success)
            .map(|item| safe_text(&item.text)),
        1 => Some("1 action completed".into()),
        count => Some(format!("{count} actions completed")),
    };

    if width < 52 {
        let available = usize::from(width.saturating_sub(2));
        let compact = match (primary, completed_copy.as_deref()) {
            (Some(item), _) if completed > 0 => {
                let suffix = format!(" · {completed} done");
                let leading = format!("{} {}", icon(item.tone), safe_text(&item.text));
                format!(
                    "{}{}",
                    truncate_display(&leading, available.saturating_sub(suffix.len())),
                    suffix
                )
            }
            (Some(item), _) => format!("{} {}", icon(item.tone), safe_text(&item.text)),
            (None, Some(done)) => format!("✓ {done}"),
            (None, None) => return Vec::new(),
        };
        return vec![Line::styled(
            format!("  {}", truncate_display(&compact, available)),
            primary.map_or(colors.muted, |item| tone_style(item.tone, colors)),
        )];
    }

    let mut lines = Vec::with_capacity(2);
    if let Some(done) = completed_copy {
        lines.push(activity_line("✓", done, colors.success, colors.muted));
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
        assert_eq!(standard[0].to_string(), "  ✓  2 actions completed");
        assert_eq!(standard[1].to_string(), "  ●  Running cargo test");
        let compact = render(&items, Theme::Mono, 40);
        assert_eq!(compact.len(), 1);
        assert_eq!(compact[0].to_string(), "  ● Running cargo test · 2 done");
    }
}
