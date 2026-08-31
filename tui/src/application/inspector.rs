//! Pure, privacy-bounded projection for the optional Inspector component.

use super::{AppModel, ConnectionState, ExecutionState, FocusTarget, Overlay, TimelineTone};
use crate::input::supports_response_schema;

impl ConnectionState {
    pub(crate) const fn reconnect_attempt_limit() -> u32 {
        5
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum InspectorVariant {
    Activity,
    Recovery,
    #[default]
    Details,
}

impl InspectorVariant {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Activity => "Activity",
            Self::Recovery => "Recovery",
            Self::Details => "Details",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InspectorActivation {
    None,
    Turn { started_position: u64 },
    RetryPending,
    Reconnect,
    Suspension,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InspectorTone {
    Neutral,
    Active,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InspectorEntry {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) tone: InspectorTone,
    pub(crate) activation: InspectorActivation,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InspectorState {
    pub(crate) open: bool,
    pub(crate) variant: InspectorVariant,
    pub(crate) selected_key: Option<String>,
    pub(crate) return_focus: FocusTarget,
    pub(crate) focus_owned: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InspectorProjection {
    pub(crate) variant: InspectorVariant,
    pub(crate) entries: Vec<InspectorEntry>,
}

impl AppModel {
    pub(crate) fn inspector_projection(&self) -> InspectorProjection {
        let variant = self.inspector.variant;
        let entries = match variant {
            InspectorVariant::Activity => activity_entries(self),
            InspectorVariant::Recovery => recovery_entries(self),
            InspectorVariant::Details => details_entries(self),
        };
        InspectorProjection { variant, entries }
    }

    pub(crate) fn default_inspector_variant(&self) -> InspectorVariant {
        if !recovery_entries(self).is_empty() {
            InspectorVariant::Recovery
        } else if !activity_entries(self).is_empty() {
            InspectorVariant::Activity
        } else {
            InspectorVariant::Details
        }
    }

    pub(crate) fn select_inspector_variant(&mut self, variant: InspectorVariant) {
        self.inspector.variant = variant;
        self.inspector.selected_key = self
            .inspector_projection()
            .entries
            .first()
            .map(|entry| entry.key.clone());
    }

    pub(crate) fn inspector_selection(&self) -> usize {
        let projection = self.inspector_projection();
        self.inspector
            .selected_key
            .as_deref()
            .and_then(|key| projection.entries.iter().position(|entry| entry.key == key))
            .unwrap_or(0)
    }

    pub(crate) fn select_inspector_index(&mut self, index: usize) {
        self.inspector.selected_key = self
            .inspector_projection()
            .entries
            .get(index)
            .map(|entry| entry.key.clone());
    }

    pub(crate) fn open_inspector(&mut self, variant: InspectorVariant) {
        if matches!(
            self.focus,
            FocusTarget::Composer | FocusTarget::Conversation
        ) {
            self.inspector.return_focus = self.focus;
        }
        self.inspector.open = true;
        self.inspector.focus_owned = true;
        self.select_inspector_variant(variant);
        self.reconcile_inspector_surface();
    }

    pub(crate) fn close_inspector(&mut self) {
        self.inspector.open = false;
        self.inspector.selected_key = None;
        self.inspector.focus_owned = false;
        if self.overlay == Some(Overlay::Inspector) {
            self.overlay = None;
        }
        if matches!(self.focus, FocusTarget::Inspector | FocusTarget::Overlay) {
            self.focus = self.inspector.return_focus;
        }
    }

    pub(crate) fn reconcile_inspector_surface(&mut self) {
        if !self.inspector.open {
            return;
        }
        match self.terminal_size.width {
            120.. => {
                if self.overlay == Some(Overlay::Inspector) {
                    self.overlay = None;
                }
                if self.overlay.is_none() && self.inspector.focus_owned {
                    self.focus = FocusTarget::Inspector;
                }
            }
            40..=119 => {
                if self.overlay.is_none() || self.overlay == Some(Overlay::Inspector) {
                    self.inspector.focus_owned = true;
                    self.overlay = Some(Overlay::Inspector);
                    self.focus = FocusTarget::Overlay;
                }
            }
            _ => {
                if self.overlay == Some(Overlay::Inspector) {
                    self.overlay = None;
                }
                if matches!(self.focus, FocusTarget::Inspector | FocusTarget::Overlay) {
                    self.focus = self.inspector.return_focus;
                }
            }
        }
    }
}

impl InspectorProjection {
    pub(crate) fn window(&self, selected: usize, capacity: usize) -> (usize, usize) {
        if self.entries.is_empty() || capacity == 0 {
            return (0, 0);
        }
        let selected = selected.min(self.entries.len() - 1);
        let start = selected
            .saturating_add(1)
            .saturating_sub(capacity)
            .min(self.entries.len().saturating_sub(capacity));
        (start, (start + capacity).min(self.entries.len()))
    }
}

fn activity_entries(model: &AppModel) -> Vec<InspectorEntry> {
    model
        .turn_blocks
        .iter()
        .flat_map(|block| {
            block.activities.iter().map(|activity| InspectorEntry {
                key: format!("activity:{}", activity.stable_key),
                label: activity.text.clone(),
                detail: activity_state(activity.tone).into(),
                tone: activity.tone.into(),
                activation: InspectorActivation::Turn {
                    started_position: block.user.position,
                },
            })
        })
        .collect()
}

fn recovery_entries(model: &AppModel) -> Vec<InspectorEntry> {
    let mut entries = Vec::with_capacity(4);
    if model.pending_recovery.current_session {
        entries.push(entry(
            "recovery:pending",
            "Command result unknown",
            "Review durable truth before exact retry.",
            InspectorTone::Warning,
            InspectorActivation::RetryPending,
        ));
    }
    if model.pending_recovery.other_session {
        entries.push(entry(
            "recovery:other-session",
            "Another Session needs review",
            "Switch Sessions to review its durable command result.",
            InspectorTone::Warning,
            InspectorActivation::None,
        ));
    }
    if let Some(suspension) = model.suspension.as_ref() {
        let activation = if matches!(
            suspension.kind.as_str(),
            "approval_required" | "external_input_required"
        ) && suspension
            .response_schema_json
            .as_deref()
            .is_some_and(supports_response_schema)
        {
            InspectorActivation::Suspension
        } else {
            InspectorActivation::None
        };
        entries.push(entry(
            "recovery:suspension",
            "Action required",
            "The selected Turn requires attention.",
            InspectorTone::Warning,
            activation,
        ));
    }
    match model.connection {
        ConnectionState::Disconnected { attempt } => entries.push(entry(
            "recovery:connection",
            format!(
                "Updates paused · attempt {attempt}/{}",
                ConnectionState::reconnect_attempt_limit()
            ),
            "Durable Turn state may be newer. Enter to resume events safely.",
            InspectorTone::Warning,
            InspectorActivation::Reconnect,
        )),
        ConnectionState::Reconnecting { attempt } => entries.push(entry(
            "recovery:connection",
            format!(
                "Reconnecting · attempt {attempt}/{}",
                ConnectionState::reconnect_attempt_limit()
            ),
            "Updates remain paused. Wait for this attempt, or use /status for details.",
            InspectorTone::Warning,
            InspectorActivation::None,
        )),
        ConnectionState::Unavailable { .. } => entries.push(entry(
            "recovery:connection",
            "Host unavailable",
            "Durable Session truth cannot be loaded. Enter to try /reconnect safely.",
            InspectorTone::Danger,
            InspectorActivation::Reconnect,
        )),
        ConnectionState::Connecting | ConnectionState::Online => {}
    }
    if model.execution == ExecutionState::Failed {
        entries.push(entry(
            "recovery:failed",
            "Turn failed",
            "Review the durable terminal state before continuing.",
            InspectorTone::Danger,
            InspectorActivation::None,
        ));
    }
    entries
}

fn details_entries(model: &AppModel) -> Vec<InspectorEntry> {
    let session = model.selected_session.as_deref().and_then(|selected| {
        model
            .sessions
            .iter()
            .position(|session| session.session_id == selected)
    });
    vec![
        entry(
            "details:connection",
            "Connection",
            connection_detail(model.connection),
            connection_tone(model.connection),
            InspectorActivation::None,
        ),
        entry(
            "details:session",
            "Session",
            session.map_or_else(
                || "None selected".into(),
                |index| format!("Session {}", index + 1),
            ),
            InspectorTone::Neutral,
            InspectorActivation::None,
        ),
        entry(
            "details:execution",
            "Execution",
            execution_detail(model.execution),
            execution_tone(model.execution),
            InspectorActivation::None,
        ),
        entry(
            "details:turns",
            "Loaded Turns",
            model.turn_blocks.len().to_string(),
            InspectorTone::Neutral,
            InspectorActivation::None,
        ),
        entry(
            "details:follow",
            "Transcript",
            if model.viewport.follow_latest {
                "Following latest"
            } else {
                "Reading earlier content"
            },
            InspectorTone::Neutral,
            InspectorActivation::None,
        ),
    ]
}

fn entry(
    key: &str,
    label: impl Into<String>,
    detail: impl Into<String>,
    tone: InspectorTone,
    activation: InspectorActivation,
) -> InspectorEntry {
    InspectorEntry {
        key: key.into(),
        label: label.into(),
        detail: detail.into(),
        tone,
        activation,
    }
}

fn activity_state(tone: TimelineTone) -> &'static str {
    match tone {
        TimelineTone::Neutral => "Updated",
        TimelineTone::Active => "Running",
        TimelineTone::Success => "Completed",
        TimelineTone::Warning => "Needs attention",
        TimelineTone::Danger => "Failed",
    }
}

impl From<TimelineTone> for InspectorTone {
    fn from(value: TimelineTone) -> Self {
        match value {
            TimelineTone::Neutral => Self::Neutral,
            TimelineTone::Active => Self::Active,
            TimelineTone::Success => Self::Success,
            TimelineTone::Warning => Self::Warning,
            TimelineTone::Danger => Self::Danger,
        }
    }
}

fn connection_detail(value: ConnectionState) -> String {
    match value {
        ConnectionState::Connecting => "Connecting · loading durable Session truth".into(),
        ConnectionState::Online => "Online · durable events current".into(),
        ConnectionState::Disconnected { attempt } => format!(
            "Disconnected · attempt {attempt}/{} · durable Turn state may be newer",
            ConnectionState::reconnect_attempt_limit()
        ),
        ConnectionState::Reconnecting { attempt } => format!(
            "Reconnecting · attempt {attempt}/{} · updates paused",
            ConnectionState::reconnect_attempt_limit()
        ),
        ConnectionState::Unavailable { .. } => {
            "Unavailable · durable Session truth cannot be loaded".into()
        }
    }
}

fn connection_tone(value: ConnectionState) -> InspectorTone {
    match value {
        ConnectionState::Online => InspectorTone::Success,
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. } => InspectorTone::Active,
        ConnectionState::Disconnected { .. } => InspectorTone::Warning,
        ConnectionState::Unavailable { .. } => InspectorTone::Danger,
    }
}

fn execution_detail(value: ExecutionState) -> &'static str {
    match value {
        ExecutionState::Idle => "Idle",
        ExecutionState::Following => "Agent running",
        ExecutionState::Suspended => "Action required",
        ExecutionState::Failed => "Failed",
    }
}

fn execution_tone(value: ExecutionState) -> InspectorTone {
    match value {
        ExecutionState::Idle => InspectorTone::Neutral,
        ExecutionState::Following => InspectorTone::Active,
        ExecutionState::Suspended => InspectorTone::Warning,
        ExecutionState::Failed => InspectorTone::Danger,
    }
}
