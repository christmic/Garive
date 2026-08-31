use garive_host_client::SessionSummary;
use ratatui::text::{Line, Span};

use super::{
    agent_label,
    style::{session_state_icon, session_state_style, Palette},
};

pub(super) fn picker_line(
    session: &SessionSummary,
    ordinal: usize,
    selected: bool,
    colors: Palette,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let state = state(session);
    Line::from(vec![
        Span::styled(format!("{marker} "), colors.selected),
        Span::styled(
            format!("Session {ordinal} · {}   ", agent_label()),
            colors.normal,
        ),
        Span::styled(
            format!("{} {state}", session_state_icon(state)),
            session_state_style(state, colors),
        ),
    ])
}

fn state(session: &SessionSummary) -> &str {
    session.latest_turn_state.as_deref().unwrap_or("new")
}
