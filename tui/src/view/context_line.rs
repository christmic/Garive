//! One-row task identity and exceptional state context.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::Widget,
};

use crate::{
    application::{AppModel, ConnectionState, ExecutionState},
    Theme,
};

use super::{motion::status_motion, palette, short_id, MotionFrame};

pub(super) fn render(
    model: &AppModel,
    theme: Theme,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
) {
    if area.is_empty() {
        return;
    }
    let colors = palette(theme);
    let state_width = if area.width >= 52 { 20 } else { 12 };
    let cells =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(state_width)]).split(area);
    let session = model
        .selected_session
        .as_deref()
        .and_then(|selected| {
            model
                .sessions
                .iter()
                .position(|session| session.session_id == selected)
        })
        .map(|index| format!("Session {}", index + 1))
        .unwrap_or_else(|| "New conversation".into());
    let definition = model
        .definitions
        .first()
        .map(|item| short_id(&item.definition_id))
        .unwrap_or("Garive");
    let mut identity = vec![Span::styled(session, colors.normal)];
    if area.width >= 80 {
        identity.push(Span::styled(format!("  ·  {definition}"), colors.muted));
    }
    Line::from(identity).render(cells[0], buffer);

    let state = exceptional_state(model, status_motion(model, motion));
    Line::styled(state, state_style(model, colors))
        .alignment(Alignment::Right)
        .render(cells[1], buffer);
}

fn exceptional_state(model: &AppModel, motion: super::motion::StatusMotion) -> String {
    match model.connection {
        ConnectionState::Online => match model.execution {
            ExecutionState::Idle => String::new(),
            ExecutionState::Following => motion.execution_label,
            ExecutionState::Suspended => "Action required".into(),
            ExecutionState::Failed => "Failed".into(),
        },
        value => super::style::connection_name(value).to_owned(),
    }
}

fn state_style(model: &AppModel, colors: super::style::Palette) -> ratatui::style::Style {
    if model.connection != ConnectionState::Online {
        return super::style::connection_style(model.connection, colors);
    }
    super::style::execution_style(model.execution, colors)
}
