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

use super::{agent_label, palette};

pub(super) fn visible(model: &AppModel) -> bool {
    model.connection != ConnectionState::Online
        || matches!(
            model.execution,
            ExecutionState::Suspended | ExecutionState::Failed
        )
}

pub(super) fn render(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
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
    let agent = if selected.is_some() || !model.definitions.is_empty() {
        agent_label()
    } else {
        "Garive"
    };
    let empty_workbench = model.turn_blocks.is_empty() && model.live_answer.current().is_none();
    let mut identity = Vec::new();
    if !empty_workbench {
        identity.push(Span::styled(session, colors.normal));
        if area.width >= 80 {
            identity.push(Span::styled(format!("  ·  {agent}"), colors.muted));
        }
    }
    Line::from(identity).render(cells[0], buffer);

    let state = exceptional_state(model);
    Line::styled(state, state_style(model, colors))
        .alignment(Alignment::Right)
        .render(cells[1], buffer);
}

fn exceptional_state(model: &AppModel) -> String {
    match model.connection {
        ConnectionState::Online => online_state(model.execution),
        value => super::style::connection_name(value).to_owned(),
    }
}

fn online_state(execution: ExecutionState) -> String {
    match execution {
        ExecutionState::Idle => String::new(),
        ExecutionState::Following => String::new(),
        ExecutionState::Suspended => "Action required".into(),
        ExecutionState::Failed => "Failed".into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_line_keeps_running_state_near_the_composer() {
        assert_eq!(online_state(ExecutionState::Following), "");
        assert_eq!(online_state(ExecutionState::Suspended), "Action required");
        assert_eq!(online_state(ExecutionState::Failed), "Failed");
    }
}
