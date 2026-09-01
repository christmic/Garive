//! Empty transcript presentation without duplicating ContextLine or Composer.

use ratatui::text::Line;

use crate::application::BootState;

use super::super::style::Palette;

pub(super) fn render(boot: BootState, _width: u16, colors: Palette) -> Vec<Line<'static>> {
    match boot {
        BootState::Cold | BootState::Loading | BootState::Ready => Vec::new(),
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
    fn ordinary_empty_states_leave_the_transcript_quiet() {
        let colors = super::super::super::palette(Theme::Dark);
        for boot in [BootState::Cold, BootState::Loading, BootState::Ready] {
            let rendered = render(boot, 100, colors)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(rendered.is_empty());
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
