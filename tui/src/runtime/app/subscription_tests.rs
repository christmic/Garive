use std::future;

use garive_host_client::{HostClientErrorCode, HostEvent, LiveOutputEvent, LiveOutputEventKind};

use crate::application::{ConnectionState, ExecutionState};

use super::{
    handle_host,
    host::{HostMessage, LiveSubscriptionId, SubscriptionId},
    RuntimeState,
};

fn pending_task() -> tokio::task::JoinHandle<()> {
    tokio::spawn(future::pending())
}

fn durable_event(position: u64) -> HostEvent {
    HostEvent {
        api_version: "garive.host.v1".into(),
        session_id: "session-a".into(),
        position,
        event: "activity.updated".into(),
        turn_id: "turn-a".into(),
        execution_id: "execution-a".into(),
        text: String::new(),
        activity: None,
    }
}

fn live_event() -> LiveOutputEvent {
    LiveOutputEvent {
        api_version: "garive.host.v1".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
        execution_id: "execution-a".into(),
        stream_id: "stream-a".into(),
        sequence: 1,
        kind: LiveOutputEventKind::TextDelta {
            text: "hello".into(),
        },
    }
}

#[tokio::test]
async fn durable_follow_rejects_old_same_session_messages() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model.selected_session = Some("session-a".into());
    state.model.connection = ConnectionState::Online;
    state.model.execution = ExecutionState::Following;
    state.model.observed_position = 4;
    let old = SubscriptionId::new(1);
    let current = SubscriptionId::new(2);
    state.follow = Some(pending_task());
    state.follow_owner = Some(current);

    handle_host(
        HostMessage::Event {
            subscription_id: old,
            event: durable_event(5),
        },
        &mut state,
    );
    handle_host(
        HostMessage::FollowEnded {
            subscription_id: old,
            session_id: "session-a".into(),
            code: HostClientErrorCode::InvalidEvent,
        },
        &mut state,
    );
    state.reconnect = Some(pending_task());
    state.reconnect_owner = Some(current);
    state.reconnect_attempt = 1;
    handle_host(
        HostMessage::ReconnectDue {
            subscription_id: old,
            session_id: "session-a".into(),
            attempt: 1,
        },
        &mut state,
    );

    assert_eq!(state.model.observed_position, 4);
    assert_eq!(state.model.connection, ConnectionState::Online);
    assert_eq!(state.follow_owner, Some(current));
    assert!(state.follow.is_some());
    assert_eq!(state.reconnect_owner, Some(current));
    assert!(state.reconnect.is_some());

    handle_host(
        HostMessage::Event {
            subscription_id: current,
            event: durable_event(5),
        },
        &mut state,
    );
    assert_eq!(state.model.observed_position, 5);
    state.stop_tasks();
}

#[tokio::test]
async fn live_follow_rejects_old_same_session_messages() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.model.selected_session = Some("session-a".into());
    state.model.selected_turn = Some("turn-a".into());
    state.model.active_execution_id = Some("execution-a".into());
    state.model.execution = ExecutionState::Following;
    let old = LiveSubscriptionId::new(1);
    let current = LiveSubscriptionId::new(2);
    state.live_follow = Some(pending_task());
    state.live_follow_owner = Some(current);

    handle_host(
        HostMessage::LiveOutput {
            subscription_id: old,
            event: live_event(),
        },
        &mut state,
    );
    handle_host(
        HostMessage::LiveFollowEnded {
            subscription_id: old,
            session_id: "session-a".into(),
            code: HostClientErrorCode::TransportFailure,
        },
        &mut state,
    );
    state.live_reconnect = Some(pending_task());
    state.live_reconnect_owner = Some(current);
    state.live_reconnect_attempt = 1;
    handle_host(
        HostMessage::LiveReconnectDue {
            subscription_id: old,
            session_id: "session-a".into(),
            attempt: 1,
        },
        &mut state,
    );

    assert!(state.model.live_answer.current().is_none());
    assert_eq!(state.live_follow_owner, Some(current));
    assert!(state.live_follow.is_some());
    assert_eq!(state.live_reconnect_owner, Some(current));
    assert!(state.live_reconnect.is_some());

    handle_host(
        HostMessage::LiveOutput {
            subscription_id: current,
            event: live_event(),
        },
        &mut state,
    );
    assert!(state.model.live_answer.current().is_some());
    state.stop_tasks();
}
