use std::collections::BTreeSet;

use garive_host_client::{HostEvent, LiveOutputEvent, TurnTimelineItem};

use crate::application::{
    AppModel, ConnectionState, ConversationLandmark, ExecutionState, LiveAnswerExpectation,
    Overlay, TimelineItem, TimelineRole, TimelineTone, TurnBlock, TurnBlockKey,
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
    if event.event == "turn.started" && !event.execution_id.is_empty() {
        state.model.active_execution_id = Some(event.execution_id.clone());
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
        if upsert_activity(&mut state.model, &event.session_id, &event.turn_id, item) {
            note_detached_durable_update(&mut state.model);
        }
    }
    if matches!(
        event.event.as_str(),
        "turn.started" | "turn.completed" | "turn.failed" | "turn.stopped" | "turn.suspended"
    ) {
        if event.event != "turn.started" {
            state
                .model
                .cancel_requests
                .clear_terminal(&event.session_id, &event.turn_id);
            state.model.live_answer.await_durable_snapshot(
                &event.session_id,
                &event.turn_id,
                (!event.execution_id.is_empty()).then_some(event.execution_id.as_str()),
            );
            state.model.active_execution_id = None;
        }
        let session = event.session_id;
        state.load(session);
    }
}

fn upsert_activity(
    model: &mut AppModel,
    session_id: &str,
    turn_id: &str,
    item: TimelineItem,
) -> bool {
    let Some(block) = model
        .turn_blocks
        .iter_mut()
        .find(|block| block.key.session_id == session_id && block.key.turn_id == turn_id)
    else {
        return false;
    };
    if let Some(existing) = block
        .activities
        .iter_mut()
        .find(|value| value.stable_key == item.stable_key)
    {
        *existing = item;
    } else {
        block.activities.push(item);
        block.activities.sort_by_key(|value| value.position);
    }
    true
}

fn note_detached_durable_update(model: &mut AppModel) {
    if !model.viewport.follow_latest {
        model.viewport.newer_updates = model.viewport.newer_updates.saturating_add(1);
    }
}

pub(super) fn apply_live_output(event: LiveOutputEvent, state: &mut RuntimeState) {
    let execution_id = event.execution_id.clone();
    let expectation = LiveAnswerExpectation {
        selected_session: state.model.selected_session.as_deref().unwrap_or_default(),
        active_turn: state.model.selected_turn.as_deref(),
        active_execution: state.model.active_execution_id.as_deref(),
        detached: !state.model.viewport.follow_latest,
    };
    let effect = state.model.live_answer.apply(event, expectation);
    if effect.accepted {
        state.live_reconnect_attempt = 0;
        if let Some(task) = state.live_reconnect.take() {
            task.abort();
        }
        if state.model.active_execution_id.is_none() {
            state.model.active_execution_id = Some(execution_id);
        }
    }
    if effect.unseen_increment {
        state.model.viewport.newer_updates = state.model.viewport.newer_updates.saturating_add(1);
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
        state
            .model
            .cancel_requests
            .clear_terminal(&event.session_id, &event.turn_id);
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
        .durable_children()
        .map(|item| item.stable_key.clone())
        .collect::<BTreeSet<_>>();
    let old_max_position = model
        .durable_children()
        .map(|item| item.position)
        .max()
        .unwrap_or(0);
    let old_anchor = model.viewport.anchor_key.clone();
    let old_anchor_index = old_anchor.as_deref().and_then(|key| {
        model
            .durable_children()
            .position(|item| item.stable_key == key)
    });
    turns.sort_by_key(|turn| turn.started_position);
    model.close_turn_navigator();
    let session_id = model.selected_session.clone().unwrap_or_default();
    model.turn_blocks.clear();
    model.conversation_landmarks.clear();
    model.suspension = None;
    model.selected_turn = None;
    model.execution = ExecutionState::Idle;
    for (index, turn) in turns.into_iter().enumerate() {
        if turn.state == "running" && turn.cancellation_requested {
            model
                .cancel_requests
                .restore_accepted(session_id.clone(), turn.turn_id.clone());
        }
        model.conversation_landmarks.push(ConversationLandmark {
            ordinal: index + 1,
            started_position: turn.started_position,
            prompt_preview: public_prompt_preview(&turn.user_text),
        });
        let user = TimelineItem {
            stable_key: format!("turn:{}:user", turn.turn_id),
            position: turn.started_position,
            role: TimelineRole::User,
            tone: Default::default(),
            text: turn.user_text,
        };
        let mut activities = Vec::with_capacity(turn.activities.len());
        for activity in turn.activities {
            let (text, tone) = activity_copy(
                &activity.kind,
                &activity.label_key,
                &activity.state,
                activity.safe_code.as_deref(),
            );
            activities.push(TimelineItem {
                stable_key: format!("activity:{}:{}", turn.turn_id, activity.activity_id),
                position: activity.source_position,
                role: TimelineRole::Status,
                tone,
                text,
            });
        }
        let committed_answer = turn.completion_text.map(|text| TimelineItem {
            stable_key: format!("turn:{}:agent", turn.turn_id),
            position: turn.latest_position,
            role: TimelineRole::Agent,
            tone: Default::default(),
            text,
        });
        let outcome = outcome_item(&turn.turn_id, turn.latest_position, &turn.state);
        let turn_id = turn.turn_id;
        model.turn_blocks.push(TurnBlock {
            key: TurnBlockKey {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
            },
            user,
            activities,
            committed_answer,
            outcome,
        });
        model.selected_turn = Some(turn_id);
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
    model.reconcile_suspension_response();
    if model.suspension.is_none() && model.overlay == Some(Overlay::Suspension) {
        model.overlay = None;
        model.reconcile_inspector_surface();
    }
    if model.execution != ExecutionState::Following {
        if let (Some(session_id), Some(turn_id)) =
            (model.selected_session.clone(), model.selected_turn.clone())
        {
            model.cancel_requests.clear_terminal(&session_id, &turn_id);
            model
                .live_answer
                .durable_takeover(&session_id, &turn_id, None);
        }
        model.active_execution_id = None;
    }
    if model.viewport.follow_latest {
        model.follow_latest();
        return;
    }
    let replacement_anchor = old_anchor
        .filter(|key| model.durable_child(key).is_some())
        .or_else(|| {
            old_anchor_index.and_then(|index| {
                model
                    .durable_children()
                    .nth(index.min(model.durable_children().count().saturating_sub(1)))
                    .map(|item| item.stable_key.clone())
            })
        });
    model.viewport.anchor_key = replacement_anchor;
    model.viewport.newer_updates = model.viewport.newer_updates.saturating_add(
        model
            .durable_children()
            .filter(|item| item.position > old_max_position && !old_keys.contains(&item.stable_key))
            .count(),
    );
}

fn outcome_item(turn_id: &str, position: u64, state: &str) -> Option<TimelineItem> {
    let (tone, text) = match state {
        "suspended" => (TimelineTone::Warning, "Action required"),
        "failed" => (TimelineTone::Danger, "Turn failed"),
        "stopped" => (TimelineTone::Neutral, "Turn stopped"),
        _ => return None,
    };
    Some(TimelineItem {
        stable_key: format!("turn:{turn_id}:outcome"),
        position,
        role: TimelineRole::Status,
        tone,
        text: text.into(),
    })
}

fn public_prompt_preview(value: &str) -> String {
    let mut preview = String::new();
    let mut pending_space = false;
    for character in value.chars().take(256) {
        if character.is_whitespace() {
            pending_space = !preview.is_empty();
            continue;
        }
        let character = if character.is_control()
            || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            '�'
        } else {
            character
        };
        if pending_space {
            preview.push(' ');
            pending_space = false;
        }
        preview.push(character);
    }
    preview
}

#[cfg(test)]
mod tests;
