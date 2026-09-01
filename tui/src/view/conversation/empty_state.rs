//! Empty transcript presentation without duplicating ContextLine or Composer.

use ratatui::text::Line;

use crate::application::BootState;

use super::super::style::Palette;

pub(super) fn render(boot: BootState, width: u16, colors: Palette) -> Vec<Line<'static>> {
    match boot {
        BootState::Cold | BootState::Loading => {
            welcome_anchor("Starting workspace…", width, colors)
        }
        BootState::Ready => welcome_anchor("/ commands  ·  ? help", width, colors),
        BootState::NotConfigured => message(
            "No Agent is installed",
            "Install an Agent definition before starting a conversation.",
            colors,
        ),
        BootState::Degraded => message(
            "Recovery details are available",
            "Open /status to inspect the safe failure and reconnect.",
            colors,
        ),
    }
}

fn welcome_anchor(detail: &'static str, width: u16, colors: Palette) -> Vec<Line<'static>> {
    let border = colors.border_set();
    let pane_width = width.saturating_sub(2).clamp(28, 46);
    let inner_width = usize::from(pane_width.saturating_sub(2));
    let row = |content: &'static str, style: ratatui::style::Style| {
        let padding = inner_width.saturating_sub(content.chars().count().saturating_add(2));
        Line::from(vec![
            ratatui::text::Span::styled(border.vertical_left, colors.border),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(content, style),
            ratatui::text::Span::raw(" ".repeat(padding)),
            ratatui::text::Span::styled(border.vertical_right, colors.border),
        ])
    };
    vec![
        Line::styled(
            format!(
                "{}{}{}",
                border.top_left,
                border.horizontal_top.repeat(inner_width),
                border.top_right
            ),
            colors.border,
        ),
        row(">_ Garive", colors.empty_title),
        row(detail, colors.muted),
        Line::styled(
            format!(
                "{}{}{}",
                border.bottom_left,
                border.horizontal_bottom.repeat(inner_width),
                border.bottom_right
            ),
            colors.border,
        ),
    ]
}

fn message(title: &'static str, detail: &'static str, colors: Palette) -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::styled(format!("  {title}"), colors.empty_title),
        Line::styled(format!("  {detail}"), colors.muted),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    #[test]
    fn ordinary_empty_states_have_one_compact_workspace_anchor() {
        let colors = super::super::super::palette(Theme::Dark);
        for boot in [BootState::Cold, BootState::Loading, BootState::Ready] {
            let rendered = render(boot, 100, colors)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(rendered.contains("Garive"));
            assert_eq!(rendered.matches("Garive").count(), 1);
        }
    }

    #[test]
    fn blocked_empty_states_expose_one_specific_recovery_path() {
        let colors = super::super::super::palette(Theme::Dark);
        let missing = render(BootState::NotConfigured, 100, colors)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(missing.contains("Install an Agent definition"));

        let degraded = render(BootState::Degraded, 100, colors)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(degraded.contains("Open /status"));
        assert!(!degraded.to_lowercase().contains("unavailable"));
    }
}
