use garive_host_client::SessionSummary;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::application::{AppModel, FocusTarget};

use super::{
    primitives::selection_window,
    short_id, short_tail,
    style::{session_state_icon, session_state_style, Palette},
    turn_label,
};

const RAIL_ITEM_ROWS: u16 = 3;

pub(super) fn rail_window(model: &AppModel, list_height: u16) -> (usize, usize) {
    let capacity = usize::from(list_height.saturating_add(1) / RAIL_ITEM_ROWS).max(1);
    let focus_id = (model.focus == FocusTarget::Navigation)
        .then_some(model.navigation_selection.as_deref())
        .flatten();
    let anchor_id = focus_id.or(model.selected_session.as_deref());
    let anchor = anchor_id
        .and_then(|id| {
            model
                .sessions
                .iter()
                .position(|session| session.session_id == id)
        })
        .unwrap_or(0);
    selection_window(model.sessions.len(), anchor, capacity)
}

pub(super) fn rail_hit_test(model: &AppModel, column: u16, row: u16) -> Option<usize> {
    let size = model.terminal_size;
    if size.width < 100 {
        return None;
    }
    let rail_width = if size.width >= 160 { 34 } else { 28 };
    if column == 0 || column >= rail_width - 1 {
        return None;
    }

    // Header: 2 rows. Rail block top padding: 1 row. Rail footer: 2 rows.
    let list = Rect::new(1, 3, rail_width - 3, size.height.saturating_sub(5));
    if !list.contains((column, row).into()) {
        return None;
    }
    let (start, end) = rail_window(model, list.height);
    let index = start + usize::from((row - list.y) / RAIL_ITEM_ROWS);
    (index < end).then_some(index)
}

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
