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
        if let Some(activity) = event.activity.as_ref() {
            validate_activity(activity, event.position)?;
            if view
                .activities
                .get(&activity.activity_id)
                .is_some_and(|prior| {
                    prior.source_position >= activity.source_position || prior.terminal
                })
            {
                return Err(HostClientError::new(
                    HostClientErrorCode::EventOrderViolation,
                ));
            }
            view.activities
                .insert(activity.activity_id.clone(), activity.clone());
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
                    && !view.unknown_events.iter().any(|name| name == unknown) =>
            {
                view.unknown_events.push(unknown.to_owned());
            }
            _ => {}
        }
    }
    Ok(view)
}

fn validate_activity(activity: &HostActivity, event_position: u64) -> Result<(), HostClientError> {
    let expected_terminal = match activity.state.as_str() {
        "input_received" | "completed" | "denied" | "failed" | "cancelled" => Some(true),
        "prepared" | "waiting_for_input" | "authorized" | "running" | "attention_required" => {
            Some(false)
        }
        _ => None,
    };
    let valid = activity.api_version == "v1"
        && !activity.activity_id.is_empty()
        && !activity.kind.is_empty()
        && !activity.label_key.is_empty()
        && activity.source_position > 0
        && activity.source_position <= event_position
        && activity.safe_code.as_deref() != Some("")
        && expected_terminal.is_none_or(|terminal| terminal == activity.terminal)
        && (expected_terminal.is_some() || !activity.terminal);
    if valid {
        Ok(())
    } else {
        Err(HostClientError::new(HostClientErrorCode::InvalidEvent))
    }
}
