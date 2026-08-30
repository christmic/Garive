//! Deterministic reduction of durable H1 events.

use crate::{
    HostActivity, HostClientError, HostClientErrorCode, HostEvent, HostTerminal, HostView,
};

const KNOWN_EVENTS: [&str; 6] = [
    "session.created",
    "turn.started",
    "turn.completed",
    "turn.suspended",
    "turn.stopped",
    "turn.failed",
];

/// Reduces ordered replay/follow events without treating EOF as terminal.
pub fn reduce_host_events(
    session_id: &str,
    events: &[HostEvent],
    mut view: HostView,
    max_events: usize,
) -> Result<HostView, HostClientError> {
    if session_id.is_empty() || max_events == 0 {
        return Err(HostClientError::new(
            HostClientErrorCode::InvalidConfiguration,
        ));
    }
    if events.len() > max_events {
        return Err(HostClientError::new(
            HostClientErrorCode::EventLimitExceeded,
        ));
    }
    let saved_cursor = view.cursor;
    for event in events {
        if event.api_version != "v1" || event.session_id != session_id || event.position == 0 {
            return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
        }
        if let Some(prior) = view.seen.get(&event.position) {
            if prior != event {
                return Err(HostClientError::new(
                    HostClientErrorCode::EventOrderViolation,
                ));
            }
            continue;
        }
        if event.position <= saved_cursor {
            continue;
        }
        if event.position <= view.cursor || view.terminal.is_some() {
            return Err(HostClientError::new(
                HostClientErrorCode::EventOrderViolation,
            ));
        }
        view.cursor = event.position;
        view.seen.insert(event.position, event.clone());
        match (&event.activity, event.event.starts_with("agent.activity.")) {
            (Some(activity), true) => reduce_activity(&mut view, event, activity)?,
            (None, false) => {}
            _ => return Err(HostClientError::new(HostClientErrorCode::InvalidEvent)),
        }
        match event.event.as_str() {
            "turn.completed" => {
                view.terminal = Some(HostTerminal::Completed);
                view.text.clone_from(&event.text);
            }
            "turn.suspended" => view.terminal = Some(HostTerminal::Suspended),
            "turn.stopped" => view.terminal = Some(HostTerminal::Stopped),
            "turn.failed" => view.terminal = Some(HostTerminal::Failed),
            unknown
                if !KNOWN_EVENTS.contains(&unknown)
                    && !unknown.starts_with("agent.activity.")
                    && !view.unknown_events.iter().any(|name| name == unknown) =>
            {
                view.unknown_events.push(unknown.to_owned());
            }
            _ => {}
        }
    }
    Ok(view)
}

fn reduce_activity(
    view: &mut HostView,
    event: &HostEvent,
    activity: &HostActivity,
) -> Result<(), HostClientError> {
    validate_activity(activity, event.position)?;
    let expected_state = match event.event.as_str() {
        "agent.activity.rejected" | "agent.activity.failed" => Some("failed"),
        "agent.activity.prepared" => Some("prepared"),
        "agent.activity.input_requested" => Some("waiting_for_input"),
        "agent.activity.input_received" => Some("input_received"),
        "agent.activity.cancelled" => Some("cancelled"),
        "agent.activity.authorized" => Some("authorized"),
        "agent.activity.denied" => Some("denied"),
        "agent.activity.started" => Some("running"),
        "agent.activity.completed" => Some("completed"),
        "agent.activity.attention_required" => Some("attention_required"),
        "agent.activity.reconciled" => None,
        _ => None,
    };
    if expected_state.is_some_and(|state| activity.state != state)
        || (event.event == "agent.activity.reconciled"
            && !matches!(activity.state.as_str(), "completed" | "failed"))
    {
        return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
    }
    if let Some(prior) = view.activities.get(&activity.activity_id) {
        if prior.kind != activity.kind
            || prior.label_key != activity.label_key
            || !valid_activity_transition(&prior.state, &activity.state)
        {
            return Err(HostClientError::new(
                HostClientErrorCode::EventOrderViolation,
            ));
        }
    } else if !matches!(
        activity.state.as_str(),
        "prepared" | "waiting_for_input" | "failed"
    ) {
        return Err(HostClientError::new(
            HostClientErrorCode::EventOrderViolation,
        ));
    }
    view.activities
        .insert(activity.activity_id.clone(), activity.clone());
    Ok(())
}

pub(crate) fn validate_activity(
    value: &HostActivity,
    enclosing_position: u64,
) -> Result<(), HostClientError> {
    let known = matches!(
        value.state.as_str(),
        "prepared"
            | "waiting_for_input"
            | "input_received"
            | "authorized"
            | "running"
            | "completed"
            | "denied"
            | "failed"
            | "cancelled"
            | "attention_required"
    );
    let expected_terminal = matches!(
        value.state.as_str(),
        "input_received" | "completed" | "denied" | "failed" | "cancelled"
    );
    if value.api_version != "v1"
        || value.activity_id.is_empty()
        || value.kind.is_empty()
        || value.label_key.is_empty()
        || value.source_position == 0
        || value.source_position > enclosing_position
        || (known && value.terminal != expected_terminal)
        || (!known && value.terminal)
        || value.safe_code.as_deref() == Some("")
    {
        return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
    }
    Ok(())
}

fn valid_activity_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("prepared", "authorized" | "running" | "denied")
            | ("authorized", "running" | "denied" | "failed")
            | ("running", "completed" | "failed" | "attention_required")
            | ("attention_required", "completed" | "failed")
            | ("waiting_for_input", "input_received" | "cancelled")
    )
}
