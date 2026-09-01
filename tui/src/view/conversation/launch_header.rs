//! Compact, factual launch identity for an empty workbench.

use ratatui::text::{Line, Span};

use crate::application::AppModel;

use super::super::style::Palette;

const CARD_WIDTH: u16 = 58;

pub(super) fn render(model: &AppModel, width: u16, colors: Palette) -> Vec<Line<'static>> {
    let agent = agent_label(model);
    let session = session_label(model);
    if width < 52 {
        return vec![
            Line::from(vec![
                Span::styled(">_ ", colors.accent),
                Span::styled("Garive", colors.title),
            ]),
            Line::from(vec![
                Span::styled("Agent    ", colors.muted),
                Span::styled(agent, colors.normal),
            ]),
            Line::from(vec![
                Span::styled("Session  ", colors.muted),
                Span::styled(session, colors.normal),
            ]),
            Line::default(),
        ];
    }

    let card_width = width.clamp(4, CARD_WIDTH);
    let inner_width = usize::from(card_width.saturating_sub(2));
    let border = colors.border_set();
    let rule = border.horizontal_top.repeat(inner_width);
    let mut lines = vec![Line::styled(
        format!("{}{}{}", border.top_left, rule, border.top_right),
        colors.border,
    )];
    lines.push(card_line(
        vec![
            Span::styled(">_ ", colors.accent),
            Span::styled("Garive", colors.title),
            Span::styled(format!("  (v{})", env!("CARGO_PKG_VERSION")), colors.muted),
        ],
        inner_width,
        border.vertical_left,
        colors,
    ));
    lines.push(card_line(
        Vec::new(),
        inner_width,
        border.vertical_left,
        colors,
    ));
    lines.push(card_line(
        vec![
            Span::styled("agent:    ", colors.muted),
            Span::styled(agent, colors.normal),
        ],
        inner_width,
        border.vertical_left,
        colors,
    ));
    lines.push(card_line(
        vec![
            Span::styled("session:  ", colors.muted),
            Span::styled(session, colors.normal),
        ],
        inner_width,
        border.vertical_left,
        colors,
    ));
    lines.push(Line::styled(
        format!("{}{}{}", border.bottom_left, rule, border.bottom_right),
        colors.border,
    ));
    lines.push(Line::default());
    lines
}

pub(super) fn desired_height(model: &AppModel, width: u16) -> u16 {
    let header = if width < 52 { 4 } else { 7 };
    header
        + u16::from(matches!(
            model.boot,
            crate::application::BootState::NotConfigured | crate::application::BootState::Degraded
        )) * 3
}

fn card_line(
    mut content: Vec<Span<'static>>,
    inner_width: usize,
    vertical: &'static str,
    colors: Palette,
) -> Line<'static> {
    let used = content.iter().map(|span| span.width()).sum::<usize>();
    content.insert(0, Span::styled(vertical, colors.border));
    content.push(Span::raw(" ".repeat(inner_width.saturating_sub(used))));
    content.push(Span::styled(vertical, colors.border));
    Line::from(content)
}

fn agent_label(model: &AppModel) -> String {
    if model.selected_session.is_some() || model.definitions.len() == 1 {
        "Ready".into()
    } else if model.definitions.is_empty() {
        "Not installed".into()
    } else {
        "Choose with /new".into()
    }
}

fn session_label(model: &AppModel) -> String {
    model
        .selected_session
        .as_deref()
        .and_then(|selected| {
            model
                .sessions
                .iter()
                .position(|session| session.session_id == selected)
        })
        .map(|index| format!("Session {}", index + 1))
        .unwrap_or_else(|| "New conversation".into())
}
