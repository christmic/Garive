use garive_host_client::SessionSummary;
use ratatui::text::{Line, Span};

use super::{
    short_id, short_tail,
    style::{session_state_icon, session_state_style, Palette},
    turn_label,
};

pub(super) fn rail_lines(
    session: &SessionSummary,
    active: bool,
    focused: bool,
    colors: Palette,
) -> [Line<'static>; 2] {
    let marker = if focused {
        "›"
    } else if active {
        "▌"
    } else {
        " "
    };
    let identity_style = if active {
        colors.selected
    } else {
        colors.normal
    };
    let state = state(session);
    [
        Line::styled(
            format!(
                "{marker} {} · {}",
                label(session),
                short_tail(&session.session_id)
            ),
            identity_style,
        ),
        Line::from(vec![
            Span::styled(
                format!("  {} {state}", session_state_icon(state)),
                session_state_style(state, colors),
            ),
            Span::styled(
                format!(
                    "  ·  {} {}",
                    session.turn_count,
                    turn_label(session.turn_count)
                ),
                colors.muted,
            ),
        ]),
    ]
}

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
