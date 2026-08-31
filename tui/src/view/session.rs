use garive_host_client::SessionSummary;
use ratatui::text::{Line, Span};

use super::{
    short_id, short_tail,
    style::{session_state_icon, session_state_style, Palette},
};

pub(super) fn picker_line(
    session: &SessionSummary,
    selected: bool,
    colors: Palette,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let state = state(session);
    Line::from(vec![
        Span::styled(format!("{marker} "), colors.selected),
        Span::styled(
            format!(
                "{} · {}   ",
                label(session),
                short_tail(&session.session_id)
            ),
            colors.normal,
        ),
        Span::styled(
            format!("{} {state}", session_state_icon(state)),
            session_state_style(state, colors),
        ),
    ])
}

fn label(session: &SessionSummary) -> &str {
    short_id(&session.definition_id)
}

fn state(session: &SessionSummary) -> &str {
    session.latest_turn_state.as_deref().unwrap_or("new")
}
