use garive_host_client::{AgentDefinitionSummary, SessionSummary, SuspensionView};
use std::collections::{BTreeMap, VecDeque};

use crate::input::{
    command_matches, CommandContext, EditorState, PromptHistoryBrowser, COMMAND_PALETTE,
};

use super::{LiveAnswerProjection, TurnBlock};

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
    TurnNavigator,
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
pub(crate) struct ConversationLandmark {
    pub(crate) ordinal: usize,
    pub(crate) started_position: u64,
    pub(crate) prompt_preview: String,
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
    pub(crate) turn_filter: String,
    pub(crate) turn_selection: usize,
    pub(crate) prompt_history: Vec<String>,
    pub(crate) prompt_history_browser: PromptHistoryBrowser,
    pub(crate) history_filter: String,
    pub(crate) history_selection: usize,
    pub(crate) command_filter: String,
    pub(crate) command_selection: usize,
    pub(crate) command_suggestion_selection: usize,
    pub(crate) command_suggestion_dismissed: Option<String>,
    pub(crate) has_pending_command: bool,
    pub(crate) composer_is_frozen: bool,
    pub(crate) session_selection: usize,
    pub(crate) selected_session: Option<String>,
    pub(crate) selected_turn: Option<String>,
    pub(crate) active_execution_id: Option<String>,
    pub(crate) observed_position: u64,
    pub(crate) viewport: ViewportState,
    pub(crate) session_viewports: BTreeMap<String, ViewportState>,
    pub(crate) viewport_order: VecDeque<String>,
    pub(crate) suspension: Option<SuspensionView>,
    pub(crate) notice: Option<String>,
    pub(crate) turn_blocks: Vec<TurnBlock>,
    pub(crate) conversation_landmarks: Vec<ConversationLandmark>,
    pub(crate) live_answer: LiveAnswerProjection,
    pub(crate) execution: ExecutionState,
    pub(crate) composer: EditorState,
}

impl AppModel {
    pub(crate) fn durable_children(&self) -> impl Iterator<Item = &TimelineItem> {
        self.turn_blocks.iter().flat_map(TurnBlock::children)
    }

    pub(crate) fn durable_child(&self, stable_key: &str) -> Option<&TimelineItem> {
        self.turn_blocks
            .iter()
            .find_map(|block| block.child(stable_key))
    }

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

    pub(crate) fn matching_landmark_indices(&self) -> Vec<usize> {
        let filter = self.turn_filter.to_lowercase();
        self.conversation_landmarks
            .iter()
            .enumerate()
            .filter(|(_, landmark)| {
                filter.is_empty() || landmark.prompt_preview.to_lowercase().contains(&filter)
            })
            .map(|(index, _)| index)
            .collect()
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

    pub(crate) fn command_suggestion_draft(&self) -> Option<&str> {
        let draft = self.composer.text();
        (self.overlay.is_none()
            && self.focus == FocusTarget::Composer
            && self.terminal_size.width >= 30
            && self.terminal_size.height >= 12
            && draft.starts_with('/')
            && !draft.contains('\n')
            && draft.len() <= 128)
            .then_some(draft)
    }

    pub(crate) fn matching_command_suggestion_indices(&self) -> Vec<usize> {
        let Some(draft) = self.command_suggestion_draft() else {
            return Vec::new();
        };
        let folded = draft.to_lowercase();
        COMMAND_PALETTE
            .iter()
            .enumerate()
            .filter(|(_, command)| command.input.starts_with(&folded))
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn command_suggestions_active(&self) -> bool {
        let Some(draft) = self.command_suggestion_draft() else {
            return false;
        };
        self.command_suggestion_dismissed.as_deref() != Some(draft)
            && !self.matching_command_suggestion_indices().is_empty()
    }

    pub(crate) fn command_context(&self) -> CommandContext {
        CommandContext {
            has_installed_agent: !self.definitions.is_empty(),
            has_pending_command: self.has_pending_command,
            has_running_turn: self.execution == ExecutionState::Following,
            has_visible_completion: self
                .turn_blocks
                .iter()
                .any(|block| block.committed_answer.is_some()),
            has_selected_session: self.selected_session.is_some(),
            has_navigable_turns: self.conversation_landmarks.len() >= 2,
            has_composer_selection: self.composer.has_selection(),
            composer_is_editable: !self.composer_is_frozen,
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

    pub(crate) fn follow_latest(&mut self) {
        self.viewport = ViewportState::default();
        self.live_answer.mark_seen();
    }

    pub(crate) fn live_frame_pending(&self) -> bool {
        self.live_answer.frame_pending()
    }

    pub(crate) fn advance_live_frame(&mut self) {
        let effect = self.live_answer.advance_frame(!self.viewport.follow_latest);
        if effect.unseen_increment {
            self.viewport.newer_updates = self.viewport.newer_updates.saturating_add(1);
        }
    }

    pub(crate) fn jump_to_oldest(&mut self) {
        let Some(first) = self.turn_blocks.first() else {
            return;
        };
        self.viewport.follow_latest = false;
        self.viewport.anchor_key = Some(first.user.stable_key.clone());
        self.viewport.source_line = 0;
        self.viewport.newer_updates = 0;
    }

    pub(crate) fn jump_to_turn_position(&mut self, position: u64) -> bool {
        let Some(block) = self
            .turn_blocks
            .iter()
            .find(|block| block.user.position == position)
        else {
            return false;
        };
        let final_position = self
            .conversation_landmarks
            .last()
            .map(|landmark| landmark.started_position);
        if final_position == Some(position) {
            self.follow_latest();
        } else {
            self.viewport.follow_latest = false;
            self.viewport.anchor_key = Some(block.user.stable_key.clone());
            self.viewport.source_line = 0;
            self.viewport.newer_updates = 0;
        }
        true
    }

    pub(crate) fn close_turn_navigator(&mut self) {
        if self.overlay == Some(Overlay::TurnNavigator) {
            self.overlay = None;
            self.focus = self.prior_focus;
        }
        self.turn_filter.clear();
        self.turn_selection = 0;
    }
}
