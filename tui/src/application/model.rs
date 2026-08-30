use std::collections::BTreeMap;

use super::{EffectId, EffectKind};

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
    QuitConfirmation,
}

impl Overlay {
    pub(crate) fn is_blocking(self) -> bool {
        matches!(self, Self::Suspension | Self::UnknownCommand)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingEffect {
    pub(crate) generation: u64,
    pub(crate) kind: EffectKind,
}

#[derive(Debug, Default)]
pub(crate) struct AppModel {
    pub(crate) generation: u64,
    pub(crate) boot: BootState,
    pub(crate) focus: FocusTarget,
    pub(crate) prior_focus: FocusTarget,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) connection: ConnectionState,
    pub(crate) terminal_size: TerminalSize,
    pub(crate) terminal_focused: bool,
    pub(crate) dirty: bool,
    pub(crate) quit_requested: bool,
    pub(crate) definition_count: usize,
    pub(crate) session_count: usize,
    pub(crate) stale_result_count: u64,
    pub(crate) next_effect_id: u64,
    pub(crate) pending_effects: BTreeMap<EffectId, PendingEffect>,
}
