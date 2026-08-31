use std::collections::BTreeSet;

use garive_host_client::{HostEvent, TurnTimelineItem};

use crate::application::{
    AppModel, ConnectionState, ExecutionState, Overlay, TimelineItem, TimelineRole,
};
use crate::view::presentation::activity_copy;

use super::RuntimeState;

pub(super) fn apply_event(event: HostEvent, state: &mut RuntimeState) {
    if state.model.selected_session.as_deref() != Some(&event.session_id) {
        apply_background_event(event, state);
        return;
    }
    if event.position <= state.model.observed_position {
        return;
    }
    state.model.observed_position = event.position;
    state.reconnect_attempt = 0;
    state.model.connection = ConnectionState::Online;
    if !event.turn_id.is_empty() {
        state.model.selected_turn = Some(event.turn_id.clone());
    }
    if let Some(activity) = event.activity {
        let (text, tone) = activity_copy(
            &activity.kind,
            &activity.label_key,
            &activity.state,
            activity.safe_code.as_deref(),
        );
        let key = format!("activity:{}:{}", event.turn_id, activity.activity_id);
        let item = TimelineItem {
            stable_key: key.clone(),
            position: activity.source_position,
            role: TimelineRole::Status,
            tone,
            text,
        };
        if let Some(existing) = state
            .model
            .timeline
            .iter_mut()
            .find(|value| value.stable_key == key)
        {
            *existing = item;
        } else {
            state.model.timeline.push(item);
        }
    }
    if matches!(
        event.event.as_str(),
        "turn.started" | "turn.completed" | "turn.failed" | "turn.stopped" | "turn.suspended"
    ) {
        let session = event.session_id;
        state.load(session);
    }
}

fn apply_background_event(event: HostEvent, state: &mut RuntimeState) {
    let Some(background) = state.background_follows.get_mut(&event.session_id) else {
        return;
    };
    if event.position <= background.observed_position {
        return;
    }
    background.observed_position = event.position;
    background.attempt = 0;
    let lifecycle = match event.event.as_str() {
        "turn.started" => Some("running"),
        "turn.suspended" => Some("suspended"),
        "turn.completed" => Some("completed"),
        "turn.failed" => Some("failed"),
        "turn.stopped" => Some("stopped"),
        _ => None,
    };
    if let Some(summary) = state
        .model
        .sessions
        .iter_mut()
        .find(|value| value.session_id == event.session_id)
    {
        summary.latest_position = summary.latest_position.max(event.position);
        if !event.turn_id.is_empty() {
            summary.latest_turn_id = Some(event.turn_id.clone());
        }
        if event.event == "turn.started" {
            summary.turn_count = summary.turn_count.saturating_add(1);
        }
        if let Some(lifecycle) = lifecycle {
            summary.latest_turn_state = Some(lifecycle.into());
        }
    }
    if matches!(
        event.event.as_str(),
        "turn.suspended" | "turn.completed" | "turn.failed" | "turn.stopped"
    ) {
        let mut finished = state
            .background_follows
            .remove(&event.session_id)
            .expect("background follow remains present");
        if let Some(task) = finished.follow.take() {
            task.abort();
        }
        if let Some(task) = finished.reconnect.take() {
            task.abort();
        }
        state.model.notice = Some(if event.event == "turn.suspended" {
            "A background Session requires action.".into()
        } else {
            "A background Session reached a terminal state.".into()
        });
        state.bell_requested = state.preferences.bell;
    }
}

pub(super) fn install_timeline(model: &mut AppModel, mut turns: Vec<TurnTimelineItem>) {
    let old_keys = model
        .timeline
        .iter()
        .map(|item| item.stable_key.clone())
        .collect::<BTreeSet<_>>();
    let old_max_position = model
        .timeline
        .iter()
        .map(|item| item.position)
        .max()
        .unwrap_or(0);
    let old_anchor = model.viewport.anchor_key.clone();
    let old_anchor_index = old_anchor.as_deref().and_then(|key| {
        model
            .timeline
            .iter()
            .position(|item| item.stable_key == key)
    });
    turns.sort_by_key(|turn| turn.started_position);
    model.conversation_rail_hover = None;
    model.timeline.clear();
    model.suspension = None;
    model.selected_turn = None;
    model.execution = ExecutionState::Idle;
    for turn in turns {
        model.timeline.push(TimelineItem {
            stable_key: format!("turn:{}:user", turn.turn_id),
            position: turn.started_position,
            role: TimelineRole::User,
            tone: Default::default(),
            text: turn.user_text,
        });
        for activity in turn.activities {
            let (text, tone) = activity_copy(
                &activity.kind,
                &activity.label_key,
                &activity.state,
                activity.safe_code.as_deref(),
            );
            model.timeline.push(TimelineItem {
                stable_key: format!("activity:{}:{}", turn.turn_id, activity.activity_id),
                position: activity.source_position,
                role: TimelineRole::Status,
                tone,
                text,
            });
        }
        if let Some(text) = turn.completion_text {
            model.timeline.push(TimelineItem {
                stable_key: format!("turn:{}:agent", turn.turn_id),
                position: turn.latest_position,
                role: TimelineRole::Agent,
                tone: Default::default(),
                text,
            });
        }
        model.selected_turn = Some(turn.turn_id);
        model.execution = match turn.state.as_str() {
            "started" | "running" => ExecutionState::Following,
            "suspended" => ExecutionState::Suspended,
            "failed" => ExecutionState::Failed,
            _ => ExecutionState::Idle,
        };
        if turn.suspension.is_some() {
            model.suspension = turn.suspension;
            model.overlay = Some(Overlay::Suspension);
        }
    }
    if model.viewport.follow_latest {
        model.follow_latest();
        return;
    }
    let replacement_anchor = old_anchor
        .filter(|key| model.timeline.iter().any(|item| item.stable_key == *key))
        .or_else(|| {
            old_anchor_index.and_then(|index| {
                model
                    .timeline
                    .get(index.min(model.timeline.len().saturating_sub(1)))
                    .map(|item| item.stable_key.clone())
            })
        });
    model.viewport.anchor_key = replacement_anchor;
    model.viewport.newer_updates = model.viewport.newer_updates.saturating_add(
        model
            .timeline
            .iter()
            .filter(|item| item.position > old_max_position && !old_keys.contains(&item.stable_key))
            .count(),
    );
}
