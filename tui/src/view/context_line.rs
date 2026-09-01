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

use super::{agent_label, motion::status_motion, palette, MotionFrame};

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
    let agent = if selected.is_some() || !model.definitions.is_empty() {
        agent_label()
    } else {
        "Garive"
    };
    let mut identity = vec![Span::styled(session, colors.normal)];
    if area.width >= 80 {
        identity.push(Span::styled(format!("  ·  {agent}"), colors.muted));
    }
    Line::from(identity).render(cells[0], buffer);

    let state = exceptional_state(model, status_motion(model, motion));
    Line::styled(state, state_style(model, colors))
        .alignment(Alignment::Right)
        .render(cells[1], buffer);
}

fn exceptional_state(model: &AppModel, motion: super::motion::StatusMotion) -> String {
    match model.connection {
        ConnectionState::Online => online_state(
            model.execution,
            transcript_owns_work_indicator(model),
            motion,
        ),
        value => super::style::connection_name(value).to_owned(),
    }
}

fn transcript_owns_work_indicator(model: &AppModel) -> bool {
    model.live_answer.current().is_some()
        || model.turn_blocks.last().is_some_and(|turn| {
            turn.activities
                .iter()
                .any(|item| item.tone == crate::application::TimelineTone::Active)
        })
}

fn online_state(
    execution: ExecutionState,
    live_answer_visible: bool,
    motion: super::motion::StatusMotion,
) -> String {
    match execution {
        ExecutionState::Idle => String::new(),
        ExecutionState::Following if live_answer_visible => String::new(),
        ExecutionState::Following => motion.execution_label,
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
    use crate::application::{TimelineItem, TimelineRole, TimelineTone};

    fn timeline_item(role: TimelineRole, tone: TimelineTone) -> TimelineItem {
        TimelineItem {
            stable_key: format!("{role:?}"),
            position: 1,
            role,
            tone,
            text: "semantic work".into(),
        }
    }

    #[test]
    fn visible_semantic_work_suppresses_only_the_duplicate_following_label() {
        let motion = || super::super::motion::StatusMotion {
            execution_label: "Agent running".into(),
        };

        assert_eq!(online_state(ExecutionState::Following, true, motion()), "");
        assert_eq!(
            online_state(ExecutionState::Following, false, motion()),
            "Agent running"
        );
        assert_eq!(
            online_state(ExecutionState::Suspended, true, motion()),
            "Action required"
        );
        assert_eq!(
            online_state(ExecutionState::Failed, true, motion()),
            "Failed"
        );
    }

    #[test]
    fn active_activity_in_latest_turn_owns_the_work_indicator() {
        let mut model = AppModel::default();
        model.push_test_timeline_item(timeline_item(TimelineRole::User, TimelineTone::Neutral));
        model.push_test_timeline_item(timeline_item(TimelineRole::Status, TimelineTone::Active));

        assert!(transcript_owns_work_indicator(&model));
    }
}
