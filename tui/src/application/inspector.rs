//! Pure, privacy-bounded projection for the optional Inspector component.

use super::{AppModel, ConnectionState, ExecutionState, TimelineTone};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum InspectorVariant {
    Activity,
    Recovery,
    #[default]
    Details,
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
    if model.pending_recovery_required {
        entries.push(entry(
            "recovery:pending",
            "Command result unknown",
            "Review durable truth before exact retry.",
            InspectorTone::Warning,
            InspectorActivation::RetryPending,
        ));
    }
    if let Some(suspension) = model.suspension.as_ref() {
        let activation = if matches!(
            suspension.kind.as_str(),
            "approval_required" | "external_input_required"
        ) {
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
        ConnectionState::Disconnected { .. } | ConnectionState::Reconnecting { .. } => {
            entries.push(entry(
                "recovery:connection",
                "Connection interrupted",
                "Reload durable Session truth and resume events.",
                InspectorTone::Warning,
                InspectorActivation::Reconnect,
            ));
        }
        ConnectionState::Unavailable { .. } => entries.push(entry(
            "recovery:connection",
            "Host unavailable",
            "Durable Session truth cannot be loaded yet.",
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
    label: &str,
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

fn connection_detail(value: ConnectionState) -> &'static str {
    match value {
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Online => "Online",
        ConnectionState::Disconnected { .. } => "Disconnected; Turn state unknown",
        ConnectionState::Reconnecting { .. } => "Reconnecting",
        ConnectionState::Unavailable { .. } => "Unavailable",
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
