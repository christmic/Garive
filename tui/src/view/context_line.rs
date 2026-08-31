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
    let state_width = if area.width >= 52 { 20 } else { 16 };
    let cells =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(state_width)]).split(area);
    let selected = model.selected_session.as_deref().and_then(|selected| {
        model
            .sessions
            .iter()
            .enumerate()
            .find(|(_, session)| session.session_id == selected)
    });
    let session = match (model.selected_session.as_ref(), selected) {
        (None, _) => "New conversation".into(),
        (Some(_), Some((index, _))) => format!("Session {}", index + 1),
        (Some(_), None) => "Current session".into(),
    };
    let definition = selected
        .map(|(_, session)| short_id(&session.definition_id))
        .or_else(|| {
            model
                .definitions
                .first()
                .map(|item| short_id(&item.definition_id))
        })
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
    match model.connection {
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. } => colors.warning,
        ConnectionState::Disconnected { .. } | ConnectionState::Unavailable { .. } => colors.danger,
        ConnectionState::Online => match model.execution {
            ExecutionState::Following => colors.accent,
            ExecutionState::Suspended => colors.warning,
            ExecutionState::Failed => colors.danger,
            ExecutionState::Idle => colors.muted,
        },
    }
}
