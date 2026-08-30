use garive_host_client::{AgentDefinitionSummary, SessionSummary, SuspensionView};
use std::collections::{BTreeMap, VecDeque};

use crate::input::{command_matches, CommandContext, EditorState, COMMAND_PALETTE};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TerminalSize {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl TerminalSize {
    pub(crate) const MINIMUM: Self = Self {
        width: 20,
        height: 8,
    };

    pub(crate) fn is_supported(self) -> bool {
        self.width >= Self::MINIMUM.width && self.height >= Self::MINIMUM.height
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum FocusTarget {
    Navigation,
    Conversation,
    #[default]
    Composer,
    Overlay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Overlay {
    CommandPalette,
    Help,
    SessionPicker,
    PromptHistory,
    Suspension,
    UnknownCommand,
    ErrorDetails,
    EphemeralConfirmation,
    QuitConfirmation,
}

impl Overlay {
    pub(crate) fn is_blocking(self) -> bool {
        matches!(self, Self::Suspension | Self::UnknownCommand)
    }

    pub(crate) fn action_bindings(self) -> Option<&'static [ActionOverlayBinding]> {
        match self {
            Self::UnknownCommand => Some(UNKNOWN_RESULT_BINDINGS),
            Self::ErrorDetails => Some(CLOSE_BINDINGS),
            Self::EphemeralConfirmation => Some(EPHEMERAL_BINDINGS),
            Self::QuitConfirmation => Some(QUIT_BINDINGS),
            _ => None,
        }
    }

    pub(crate) fn action_for_key(self, key: ActionOverlayKey) -> Option<ActionOverlayIntent> {
        self.action_bindings()?
            .iter()
            .find(|binding| binding.key == key)
            .map(|binding| binding.intent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionOverlayKey {
    Enter,
    Escape,
    Character(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionOverlayIntent {
    Close,
    ConfirmQuit,
    AcceptEphemeral,
    ExactRetry,
    AbandonPending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ActionOverlayBinding {
    pub(crate) key: ActionOverlayKey,
    pub(crate) visual_key: &'static str,
    pub(crate) spoken_key: &'static str,
    pub(crate) action: &'static str,
    pub(crate) intent: ActionOverlayIntent,
}

const UNKNOWN_RESULT_BINDINGS: &[ActionOverlayBinding] = &[
    ActionOverlayBinding {
        key: ActionOverlayKey::Enter,
        visual_key: "Enter",
        spoken_key: "Enter",
        action: "exact retry",
        intent: ActionOverlayIntent::ExactRetry,
    },
    ActionOverlayBinding {
        key: ActionOverlayKey::Character('a'),
        visual_key: "A",
        spoken_key: "A",
        action: "abandon local record",
        intent: ActionOverlayIntent::AbandonPending,
    },
];
const CLOSE_BINDINGS: &[ActionOverlayBinding] = &[ActionOverlayBinding {
    key: ActionOverlayKey::Escape,
    visual_key: "Esc",
    spoken_key: "Escape",
    action: "close",
    intent: ActionOverlayIntent::Close,
}];
const EPHEMERAL_BINDINGS: &[ActionOverlayBinding] = &[
    ActionOverlayBinding {
        key: ActionOverlayKey::Enter,
        visual_key: "Enter",
        spoken_key: "Enter",
        action: "accept for this run",
        intent: ActionOverlayIntent::AcceptEphemeral,
    },
    ActionOverlayBinding {
        key: ActionOverlayKey::Escape,
        visual_key: "Esc",
        spoken_key: "Escape",
        action: "cancel",
        intent: ActionOverlayIntent::Close,
    },
];
const QUIT_BINDINGS: &[ActionOverlayBinding] = &[
    ActionOverlayBinding {
        key: ActionOverlayKey::Enter,
        visual_key: "Enter",
        spoken_key: "Enter",
        action: "quit",
        intent: ActionOverlayIntent::ConfirmQuit,
    },
    ActionOverlayBinding {
        key: ActionOverlayKey::Escape,
        visual_key: "Esc",
        spoken_key: "Escape",
        action: "keep working",
        intent: ActionOverlayIntent::Close,
    },
];

#[cfg(test)]
mod overlay_binding_tests {
    use super::*;

    #[test]
    fn every_action_overlay_binding_round_trips_to_its_controller_intent() {
        for overlay in [
            Overlay::UnknownCommand,
            Overlay::ErrorDetails,
            Overlay::EphemeralConfirmation,
            Overlay::QuitConfirmation,
        ] {
            let bindings = overlay.action_bindings().expect("action bindings");
            assert!(!bindings.is_empty());
            for binding in bindings {
                assert_eq!(overlay.action_for_key(binding.key), Some(binding.intent));
                assert!(!binding.visual_key.is_empty());
                assert!(!binding.spoken_key.is_empty());
                assert!(!binding.action.is_empty());
            }
        }
        assert!(Overlay::Help.action_bindings().is_none());
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ConnectionState {
    #[default]
    Connecting,
    Online,
    Disconnected {
        attempt: u32,
    },
    Reconnecting {
        attempt: u32,
    },
    Unavailable {
        safe_code: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BootState {
    #[default]
    Cold,
    Loading,
    Ready,
    NotConfigured,
    Degraded,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ExecutionState {
    #[default]
    Idle,
    Following,
    Suspended,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimelineRole {
    User,
    Agent,
    Status,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TimelineTone {
    #[default]
    Neutral,
    Active,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TimelineItem {
    pub(crate) stable_key: String,
    pub(crate) position: u64,
    pub(crate) role: TimelineRole,
    pub(crate) tone: TimelineTone,
    pub(crate) text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ViewportState {
    pub(crate) follow_latest: bool,
    pub(crate) anchor_key: Option<String>,
    pub(crate) source_line: usize,
    pub(crate) newer_updates: usize,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            follow_latest: true,
            anchor_key: None,
            source_line: 0,
            newer_updates: 0,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AppModel {
    pub(crate) boot: BootState,
    pub(crate) focus: FocusTarget,
    pub(crate) prior_focus: FocusTarget,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) connection: ConnectionState,
    pub(crate) terminal_size: TerminalSize,
    pub(crate) terminal_focused: bool,
    pub(crate) quit_requested: bool,
    pub(crate) definition_count: usize,
    pub(crate) definitions: Vec<AgentDefinitionSummary>,
    pub(crate) session_count: usize,
    pub(crate) sessions: Vec<SessionSummary>,
    pub(crate) sessions_next_before: Option<String>,
    pub(crate) sessions_loading: bool,
    pub(crate) session_filter: String,
    pub(crate) prompt_history: Vec<String>,
    pub(crate) history_filter: String,
    pub(crate) history_selection: usize,
    pub(crate) command_filter: String,
    pub(crate) command_selection: usize,
    pub(crate) has_pending_command: bool,
    pub(crate) session_selection: usize,
    pub(crate) navigation_selection: Option<String>,
    pub(crate) selected_session: Option<String>,
    pub(crate) selected_turn: Option<String>,
    pub(crate) observed_position: u64,
    pub(crate) viewport: ViewportState,
    pub(crate) session_viewports: BTreeMap<String, ViewportState>,
    pub(crate) viewport_order: VecDeque<String>,
    pub(crate) suspension: Option<SuspensionView>,
    pub(crate) notice: Option<String>,
    pub(crate) timeline: Vec<TimelineItem>,
    pub(crate) execution: ExecutionState,
    pub(crate) composer: EditorState,
}

impl AppModel {
    pub(crate) fn matching_sessions(&self) -> impl Iterator<Item = &SessionSummary> {
        let filter = self.session_filter.to_lowercase();
        self.sessions.iter().filter(move |session| {
            filter.is_empty()
                || session.session_id.to_lowercase().contains(&filter)
                || session.definition_id.to_lowercase().contains(&filter)
        })
    }

    pub(crate) fn matching_history(&self) -> impl Iterator<Item = &String> {
        let filter = self.history_filter.to_lowercase();
        self.prompt_history
            .iter()
            .filter(move |text| filter.is_empty() || text.to_lowercase().contains(&filter))
    }

    pub(crate) fn matching_command_indices(&self) -> Vec<usize> {
        COMMAND_PALETTE
            .iter()
            .enumerate()
            .filter(|(_, command)| {
                command_matches(command.input, command.help, &self.command_filter)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn command_context(&self) -> CommandContext {
        CommandContext {
            has_installed_agent: !self.definitions.is_empty(),
            has_pending_command: self.has_pending_command,
            has_running_turn: self.execution == ExecutionState::Following,
            has_visible_completion: self
                .timeline
                .iter()
                .any(|item| item.role == TimelineRole::Agent),
            has_selected_session: self.selected_session.is_some(),
        }
    }

    pub(crate) fn switch_viewport(&mut self, session_id: &str) {
        if self.selected_session.as_deref() == Some(session_id) {
            return;
        }
        if let Some(previous) = self.selected_session.clone() {
            self.session_viewports
                .insert(previous.clone(), self.viewport.clone());
            self.touch_viewport(previous);
        }
        self.viewport = self
            .session_viewports
            .get(session_id)
            .cloned()
            .unwrap_or_default();
        self.touch_viewport(session_id.to_owned());
        while self.viewport_order.len() > 64 {
            if let Some(evicted) = self.viewport_order.pop_front() {
                if evicted != session_id {
                    self.session_viewports.remove(&evicted);
                }
            }
        }
    }

    fn touch_viewport(&mut self, session_id: String) {
        self.viewport_order.retain(|value| value != &session_id);
        self.viewport_order.push_back(session_id);
    }

    pub(crate) fn scroll_conversation_up(&mut self, cells: usize) {
        if self.timeline.is_empty() || cells == 0 {
            return;
        }
        let current = self
            .viewport
            .anchor_key
            .as_deref()
            .and_then(|key| self.timeline.iter().position(|item| item.stable_key == key))
            .unwrap_or(self.timeline.len() - 1);
        let target = current.saturating_sub(cells);
        self.viewport.follow_latest = false;
        self.viewport.anchor_key = Some(self.timeline[target].stable_key.clone());
        self.viewport.source_line = 0;
    }

    pub(crate) fn scroll_conversation_down(&mut self, cells: usize) {
        if self.timeline.is_empty() || cells == 0 {
            return;
        }
        let current = self
            .viewport
            .anchor_key
            .as_deref()
            .and_then(|key| self.timeline.iter().position(|item| item.stable_key == key))
            .unwrap_or(self.timeline.len() - 1);
        let target = current.saturating_add(cells);
        if target >= self.timeline.len() - 1 {
            self.follow_latest();
        } else {
            self.viewport.anchor_key = Some(self.timeline[target].stable_key.clone());
            self.viewport.source_line = 0;
        }
    }

    pub(crate) fn follow_latest(&mut self) {
        self.viewport = ViewportState::default();
    }

    pub(crate) fn jump_to_oldest(&mut self) {
        let Some(first) = self.timeline.first() else {
            return;
        };
        self.viewport.follow_latest = false;
        self.viewport.anchor_key = Some(first.stable_key.clone());
        self.viewport.source_line = 0;
        self.viewport.newer_updates = 0;
    }
}
