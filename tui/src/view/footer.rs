use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    application::{AppModel, ConnectionState, FocusTarget},
    Theme,
};

use super::composer_run_rail;
use super::{palette, primitives::key_hints};

pub(super) fn render_footer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    if model.overlay.is_some() {
        return;
    }
    // Cancellation already owns the Composer's single status slot. Repeating a
    // generic frozen-state sentence here creates a competing second voice.
    if composer_run_rail::has_cancel_request(model) {
        return;
    }
    let colors = palette(theme);
    let hint = match project(model) {
        Some(HintLine::Action {
            key, verb, detail, ..
        }) => {
            let mut line = key_hints(&[(key, verb)], colors);
            if let Some(detail) = detail {
                line.spans.push(Span::styled("  ·  ", colors.muted));
                line.spans.push(Span::styled(detail, colors.muted));
            }
            line
        }
        Some(HintLine::Status { text, tone, .. }) => Line::from(vec![
            Span::styled(" ● ", tone.style(colors)),
            Span::styled(text, colors.normal),
        ]),
        None => {
            if super::context_line::visible(model) {
                return;
            }
            render_ambient_footer(model, colors, area, buffer);
            return;
        }
    };
    hint.render(area, buffer);
}

fn render_ambient_footer(
    model: &AppModel,
    colors: super::style::Palette,
    area: Rect,
    buffer: &mut Buffer,
) {
    if model.composer.text().is_empty() {
        let left = Line::from(vec![
            Span::styled("  ", colors.muted),
            Span::styled("Ctrl+P", colors.normal),
            Span::styled(" commands", colors.muted),
        ]);
        Paragraph::new(left).render(area, buffer);
    }
    if area.width >= 52 {
        if let Some(session) = ambient_session_label(model) {
            let copy = format!("{session}  ");
            let width = u16::try_from(UnicodeWidthStr::width(copy.as_str()))
                .unwrap_or(area.width)
                .min(area.width);
            let right = Rect::new(area.right().saturating_sub(width), area.y, width, 1);
            Paragraph::new(Line::styled(copy, colors.muted)).render(right, buffer);
        }
    }
}

fn ambient_session_label(model: &AppModel) -> Option<String> {
    let selected = model.selected_session.as_deref()?;
    model
        .sessions
        .iter()
        .position(|session| session.session_id == selected)
        .map(|index| format!("Session {}", index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_identity_requires_a_real_selected_session() {
        assert_eq!(ambient_session_label(&AppModel::default()), None);

        let mut model = AppModel {
            selected_session: Some("missing".into()),
            ..Default::default()
        };
        assert_eq!(ambient_session_label(&model), None);

        model.sessions.push(garive_host_client::SessionSummary {
            api_version: "v1".into(),
            session_id: "missing".into(),
            agent_instance_id: "agent".into(),
            definition_id: "definition".into(),
            definition_revision: "revision".into(),
            opened_at: "2026-09-01T00:00:00Z".into(),
            latest_position: 0,
            latest_turn_id: None,
            latest_turn_state: None,
            turn_count: 0,
        });
        assert_eq!(ambient_session_label(&model).as_deref(), Some("Session 1"));
    }

    #[test]
    fn ambient_footer_prioritizes_a_real_action_and_drops_generic_agent_copy() {
        let mut model = AppModel {
            selected_session: Some("session".into()),
            focus: FocusTarget::Composer,
            connection: ConnectionState::Online,
            ..Default::default()
        };
        model.sessions.push(garive_host_client::SessionSummary {
            api_version: "v1".into(),
            session_id: "session".into(),
            agent_instance_id: "agent".into(),
            definition_id: "definition".into(),
            definition_revision: "revision".into(),
            opened_at: "2026-09-01T00:00:00Z".into(),
            latest_position: 0,
            latest_turn_id: None,
            latest_turn_state: None,
            turn_count: 0,
        });
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        render_footer(&model, Theme::Mono, area, &mut buffer);

        let text = (0..area.width)
            .map(|column| buffer[(column, 0)].symbol())
            .collect::<String>();
        assert!(text.starts_with("  Ctrl+P commands"));
        assert!(text.ends_with("Session 1  "));
        assert!(!text.contains("Agent"));
    }

    #[test]
    fn overlay_suppresses_the_background_footer() {
        let model = AppModel {
            overlay: Some(crate::application::Overlay::Help),
            ..Default::default()
        };
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        render_footer(&model, Theme::Mono, area, &mut buffer);

        assert!((0..area.width).all(|column| buffer[(column, 0)].symbol() == " "));
    }

    #[test]
    fn active_transcript_moves_interrupt_control_into_the_hint_row() {
        let mut model = AppModel {
            execution: crate::application::ExecutionState::Following,
            ..Default::default()
        };
        model.push_test_timeline_item(crate::application::TimelineItem {
            stable_key: "user".into(),
            position: 1,
            role: crate::application::TimelineRole::User,
            tone: crate::application::TimelineTone::Neutral,
            text: "Inspect the layout".into(),
        });
        model.push_test_timeline_item(crate::application::TimelineItem {
            stable_key: "activity".into(),
            position: 2,
            role: crate::application::TimelineRole::Status,
            tone: crate::application::TimelineTone::Active,
            text: "Reading files".into(),
        });
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);

        render_footer(&model, Theme::Mono, area, &mut buffer);

        let text = (0..area.width)
            .map(|column| buffer[(column, 0)].symbol())
            .collect::<String>();
        assert!(
            text.trim_start().starts_with("Esc interrupt"),
            "rendered footer: {text:?}"
        );
        assert!(!text.contains("Working"));
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum HintPriority {
    Navigation,
    Notice,
    ByteLimit,
    Suggestion,
    Selection,
    TurnControl,
    Recovery,
}

enum HintLine {
    Action {
        priority: HintPriority,
        key: &'static str,
        verb: &'static str,
        detail: Option<String>,
    },
    Status {
        priority: HintPriority,
        text: String,
        tone: HintTone,
    },
}

impl HintLine {
    fn priority(&self) -> HintPriority {
        match self {
            Self::Action { priority, .. } | Self::Status { priority, .. } => *priority,
        }
    }
}

#[derive(Clone, Copy)]
enum HintTone {
    Notice,
    Warning,
    Danger,
}

impl HintTone {
    fn style(self, colors: super::style::Palette) -> ratatui::style::Style {
        match self {
            Self::Notice => colors.notice,
            Self::Warning => colors.warning,
            Self::Danger => colors.danger,
        }
    }
}

fn project(model: &AppModel) -> Option<HintLine> {
    if model.overlay.is_some() {
        return None;
    }
    let mut candidates = Vec::new();
    if model.pending_recovery.current_session {
        candidates.push(HintLine::Action {
            priority: HintPriority::Recovery,
            key: "Ctrl+P",
            verb: "open recovery actions",
            detail: None,
        });
    } else if model.composer_is_frozen {
        candidates.push(HintLine::Status {
            priority: HintPriority::Recovery,
            text: "Waiting for durable command truth…".into(),
            tone: HintTone::Warning,
        });
    } else {
        match model.connection {
            ConnectionState::Disconnected { attempt } => candidates.push(HintLine::Action {
                priority: HintPriority::Recovery,
                key: "/reconnect",
                verb: "resume events",
                detail: Some(format!(
                    "Updates paused · attempt {attempt}/{}",
                    ConnectionState::reconnect_attempt_limit()
                )),
            }),
            ConnectionState::Reconnecting { attempt } => candidates.push(HintLine::Action {
                priority: HintPriority::Recovery,
                key: "/status",
                verb: "view details",
                detail: Some(format!(
                    "Updates paused · attempt {attempt}/{}",
                    ConnectionState::reconnect_attempt_limit()
                )),
            }),
            ConnectionState::Unavailable { .. } => candidates.push(HintLine::Action {
                priority: HintPriority::Recovery,
                key: "/reconnect",
                verb: "try again safely",
                detail: Some("Durable Session truth unavailable".into()),
            }),
            ConnectionState::Connecting | ConnectionState::Online => {}
        }
    }
    if model.composer.has_selection() {
        candidates.push(HintLine::Action {
            priority: HintPriority::Selection,
            key: "Alt+C",
            verb: "copy selection",
            detail: None,
        });
    }
    if model.command_suggestions_active() && !model.composer_is_frozen {
        candidates.push(HintLine::Action {
            priority: HintPriority::Suggestion,
            key: "Tab",
            verb: "complete command",
            detail: None,
        });
    }
    if model.execution == crate::application::ExecutionState::Following
        && composer_run_rail::transcript_owns_work(model)
    {
        candidates.push(HintLine::Action {
            priority: HintPriority::TurnControl,
            key: "Esc",
            verb: "interrupt",
            detail: None,
        });
    }
    let bytes = model.composer.text().len();
    if bytes > 4_096 {
        candidates.push(HintLine::Status {
            priority: HintPriority::ByteLimit,
            text: format!("Message is {} bytes over the limit", bytes - 4_096),
            tone: HintTone::Danger,
        });
    } else if bytes > 3_584 {
        candidates.push(HintLine::Status {
            priority: HintPriority::ByteLimit,
            text: format!("{bytes} of 4096 bytes"),
            tone: HintTone::Warning,
        });
    }
    if let Some(notice) = model.notice.as_deref() {
        candidates.push(HintLine::Status {
            priority: HintPriority::Notice,
            text: notice.into(),
            tone: HintTone::Notice,
        });
    }
    match (model.focus, model.viewport.follow_latest) {
        (FocusTarget::Conversation, false) => candidates.push(HintLine::Action {
            priority: HintPriority::Navigation,
            key: "End",
            verb: "follow latest",
            detail: None,
        }),
        (FocusTarget::Conversation, true) => candidates.push(HintLine::Action {
            priority: HintPriority::Navigation,
            key: "PgUp",
            verb: "browse history",
            detail: None,
        }),
        _ => {}
    }
    candidates.into_iter().max_by_key(HintLine::priority)
}
