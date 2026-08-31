use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::Widget,
};

use crate::{
    application::{AppModel, ConnectionState, ExecutionState, FocusTarget},
    Theme,
};

use super::{palette, primitives::key_hints};

pub(super) fn render_footer(model: &AppModel, theme: Theme, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    let colors = palette(theme);
    let hint = match project(model) {
        Some(HintLine::Action { key, verb, .. }) => key_hints(&[(key, verb)], colors),
        Some(HintLine::Status { text, tone, .. }) => Line::from(vec![
            Span::styled(" ● ", tone.style(colors)),
            Span::styled(text, colors.normal),
        ]),
        None => Line::default(),
    };
    hint.render(area, buffer);
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum HintPriority {
    Navigation,
    Notice,
    ByteLimit,
    Suggestion,
    Selection,
    Cancellation,
    Recovery,
}

enum HintLine {
    Action {
        priority: HintPriority,
        key: &'static str,
        verb: &'static str,
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
    if model
        .overlay
        .is_some_and(crate::application::Overlay::is_blocking)
    {
        return None;
    }
    let mut candidates = Vec::new();
    if model.pending_recovery.current_session {
        candidates.push(HintLine::Action {
            priority: HintPriority::Recovery,
            key: "Ctrl+P",
            verb: "open recovery actions",
        });
    } else if model.composer_is_frozen {
        candidates.push(HintLine::Status {
            priority: HintPriority::Recovery,
            text: "Waiting for durable command truth…".into(),
            tone: HintTone::Warning,
        });
    } else {
        match model.connection {
            ConnectionState::Disconnected { .. } => candidates.push(HintLine::Action {
                priority: HintPriority::Recovery,
                key: "Ctrl+P",
                verb: "reconnect safely",
            }),
            ConnectionState::Reconnecting { attempt } => candidates.push(HintLine::Status {
                priority: HintPriority::Recovery,
                text: format!("Reconnecting ({attempt}/5)…"),
                tone: HintTone::Warning,
            }),
            ConnectionState::Unavailable { .. } => candidates.push(HintLine::Action {
                priority: HintPriority::Recovery,
                key: "Ctrl+P",
                verb: "open recovery details",
            }),
            ConnectionState::Connecting | ConnectionState::Online => {}
        }
    }
    if model.execution == ExecutionState::Following {
        candidates.push(HintLine::Action {
            priority: HintPriority::Cancellation,
            key: "Esc",
            verb: "cancel run",
        });
    }
    if model.composer.has_selection() {
        candidates.push(HintLine::Action {
            priority: HintPriority::Selection,
            key: "Alt+C",
            verb: "copy selection",
        });
    }
    if model.command_suggestions_active() && !model.composer_is_frozen {
        candidates.push(HintLine::Action {
            priority: HintPriority::Suggestion,
            key: "Tab",
            verb: "complete command",
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
        }),
        (FocusTarget::Conversation, true) => candidates.push(HintLine::Action {
            priority: HintPriority::Navigation,
            key: "PgUp",
            verb: "browse history",
        }),
        _ => {}
    }
    candidates.into_iter().max_by_key(HintLine::priority)
}
