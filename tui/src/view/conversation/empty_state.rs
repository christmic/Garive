//! Empty transcript presentation without duplicating ContextLine or Composer.

use ratatui::text::Line;

use crate::application::{AppModel, BootState};

use super::super::style::Palette;
use super::launch_header;

pub(super) fn render(model: &AppModel, width: u16, colors: Palette) -> Vec<Line<'static>> {
    let mut lines = launch_header::render(model, width, colors);
    match model.boot {
        BootState::Cold | BootState::Loading | BootState::Ready => {}
        BootState::NotConfigured => message(
            "No Agent is installed",
            "Install an Agent definition before starting a conversation.",
            colors,
        )
        .into_iter()
        .for_each(|line| lines.push(line)),
        BootState::Degraded => message(
            "Recovery details are available",
            "Open /status to inspect the safe failure and reconnect.",
            colors,
        )
        .into_iter()
        .for_each(|line| lines.push(line)),
    }
    lines
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
    fn ordinary_empty_states_keep_one_factual_launch_identity() {
        let colors = super::super::super::palette(Theme::Dark);
        for boot in [BootState::Cold, BootState::Loading, BootState::Ready] {
            let model = AppModel {
                boot,
                ..Default::default()
            };
            let rendered = render(&model, 100, colors)
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(rendered.matches("Garive").count(), 1);
            assert!(rendered.contains("New conversation"));
        }
    }

    #[test]
    fn blocked_empty_states_expose_one_specific_recovery_path() {
        let colors = super::super::super::palette(Theme::Dark);
        let missing = render(
            &AppModel {
                boot: BootState::NotConfigured,
                ..Default::default()
            },
            100,
            colors,
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(missing.contains("Install an Agent definition"));

        let degraded = render(
            &AppModel {
                boot: BootState::Degraded,
                ..Default::default()
            },
            100,
            colors,
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(degraded.contains("Open /status"));
        assert!(!degraded.to_lowercase().contains("unavailable"));
    }
}
