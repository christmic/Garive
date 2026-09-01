use std::{
    future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use garive_host_client::HostClientErrorCode;
use tokio::sync::mpsc;

use crate::application::ExecutionState;

use super::super::{
    handle_host,
    host::{HostMessage, LiveSubscriptionId, SubscriptionId},
    RuntimeState,
};

fn pending_task() -> tokio::task::JoinHandle<()> {
    tokio::spawn(future::pending())
}

fn state_with_messages() -> (RuntimeState, mpsc::Receiver<HostMessage>) {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    let (sender, receiver) = mpsc::channel(32);
    state.sender = sender;
    (state, receiver)
}

async fn reconnect_message(
    receiver: &mut mpsc::Receiver<HostMessage>,
    delay_ms: u64,
) -> HostMessage {
    tokio::task::yield_now().await;
    assert!(receiver.try_recv().is_err());
    tokio::time::advance(Duration::from_millis(delay_ms - 1)).await;
    assert!(receiver.try_recv().is_err());
    tokio::time::advance(Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    receiver.try_recv().expect("reconnect deadline fires once")
}

async fn assert_no_sixth_message(receiver: &mut mpsc::Receiver<HostMessage>) {
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::task::yield_now().await;
    assert!(receiver.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn durable_reconnect_runs_exact_five_backoffs_and_rejects_stale_timer() {
    let (mut state, mut receiver) = state_with_messages();
    state.model.selected_session = Some("session-a".into());
    state.model.execution = ExecutionState::Following;
    let mut owner = SubscriptionId::new(100);
    state.follow = Some(pending_task());
    state.follow_owner = Some(owner);

    for (attempt, delay_ms) in [250, 500, 1_000, 2_000, 4_000].into_iter().enumerate() {
        state.follow.as_ref().unwrap().abort();
        handle_host(
            HostMessage::FollowEnded {
                subscription_id: owner,
                session_id: "session-a".into(),
                code: HostClientErrorCode::TransportFailure,
            },
            &mut state,
        );
        let attempt = u32::try_from(attempt + 1).unwrap();
        assert_eq!(state.reconnect_attempt, attempt);
        assert_eq!(state.reconnect_owner, Some(owner));

        if attempt == 1 {
            for (subscription_id, stale_attempt) in
                [(SubscriptionId::new(999), attempt), (owner, 2)]
            {
                handle_host(
                    HostMessage::ReconnectDue {
                        subscription_id,
                        session_id: "session-a".into(),
                        attempt: stale_attempt,
                    },
                    &mut state,
                );
            }
            assert!(state.reconnect.is_some());
            assert_eq!(state.reconnect_owner, Some(owner));
        }

        let message = reconnect_message(&mut receiver, delay_ms).await;
        assert!(matches!(
            &message,
            HostMessage::ReconnectDue { subscription_id, session_id, attempt: due }
                if *subscription_id == owner && session_id == "session-a" && *due == attempt
        ));
        handle_host(message, &mut state);
        owner = state.follow_owner.expect("new durable owner");
        state.follow.as_ref().unwrap().abort();
        state.follow = Some(pending_task());
    }

    state.follow.as_ref().unwrap().abort();
    handle_host(
        HostMessage::FollowEnded {
            subscription_id: owner,
            session_id: "session-a".into(),
            code: HostClientErrorCode::TransportFailure,
        },
        &mut state,
    );
    assert_eq!(state.reconnect_attempt, 5);
    assert!(state.reconnect.is_none());
    assert_no_sixth_message(&mut receiver).await;
    state.stop_tasks();
}

#[tokio::test(start_paused = true)]
async fn live_reconnect_runs_exact_five_backoffs_and_rejects_stale_timer() {
    let (mut state, mut receiver) = state_with_messages();
    state.model.selected_session = Some("session-a".into());
    state.model.execution = ExecutionState::Following;
    let mut owner = LiveSubscriptionId::new(100);
    state.live_follow = Some(pending_task());
    state.live_follow_owner = Some(owner);

    for (attempt, delay_ms) in [250, 500, 1_000, 2_000, 4_000].into_iter().enumerate() {
        state.live_follow.as_ref().unwrap().abort();
        handle_host(
            HostMessage::LiveFollowEnded {
                subscription_id: owner,
                session_id: "session-a".into(),
                code: HostClientErrorCode::TransportFailure,
            },
            &mut state,
        );
        let attempt = u32::try_from(attempt + 1).unwrap();
        assert_eq!(state.live_reconnect_attempt, attempt);
        assert_eq!(state.live_reconnect_owner, Some(owner));

        if attempt == 1 {
            for (subscription_id, stale_attempt) in
                [(LiveSubscriptionId::new(999), attempt), (owner, 2)]
            {
                handle_host(
                    HostMessage::LiveReconnectDue {
                        subscription_id,
                        session_id: "session-a".into(),
                        attempt: stale_attempt,
                    },
                    &mut state,
                );
            }
            assert!(state.live_reconnect.is_some());
            assert_eq!(state.live_reconnect_owner, Some(owner));
        }

        let message = reconnect_message(&mut receiver, delay_ms).await;
        assert!(matches!(
            &message,
            HostMessage::LiveReconnectDue { subscription_id, session_id, attempt: due }
                if *subscription_id == owner && session_id == "session-a" && *due == attempt
        ));
        handle_host(message, &mut state);
        owner = state.live_follow_owner.expect("new live owner");
        state.live_follow.as_ref().unwrap().abort();
        state.live_follow = Some(pending_task());
    }

    state.live_follow.as_ref().unwrap().abort();
    handle_host(
        HostMessage::LiveFollowEnded {
            subscription_id: owner,
            session_id: "session-a".into(),
            code: HostClientErrorCode::TransportFailure,
        },
        &mut state,
    );
    assert_eq!(state.live_reconnect_attempt, 5);
    assert!(state.live_reconnect.is_none());
    assert_no_sixth_message(&mut receiver).await;
    state.stop_tasks();
}

#[tokio::test(start_paused = true)]
async fn background_reconnect_runs_exact_five_backoffs_and_rejects_stale_timer() {
    let (mut state, mut receiver) = state_with_messages();
    state.model.selected_session = Some("selected".into());
    let mut owner = SubscriptionId::new(100);
    state.add_background_follow_for_test("session-a".into(), 7, owner, pending_task());

    for (attempt, delay_ms) in [250, 500, 1_000, 2_000, 4_000].into_iter().enumerate() {
        state.background_follows["session-a"]
            .follow
            .as_ref()
            .unwrap()
            .abort();
        handle_host(
            HostMessage::FollowEnded {
                subscription_id: owner,
                session_id: "session-a".into(),
                code: HostClientErrorCode::TransportFailure,
            },
            &mut state,
        );
        let attempt = u32::try_from(attempt + 1).unwrap();
        let background = &state.background_follows["session-a"];
        assert_eq!(background.attempt, attempt);
        assert_eq!(background.reconnect_owner, Some(owner));

        if attempt == 1 {
            for (subscription_id, stale_attempt) in
                [(SubscriptionId::new(999), attempt), (owner, 2)]
            {
                handle_host(
                    HostMessage::ReconnectDue {
                        subscription_id,
                        session_id: "session-a".into(),
                        attempt: stale_attempt,
                    },
                    &mut state,
                );
            }
            assert!(state.background_follows["session-a"].reconnect.is_some());
            assert_eq!(
                state.background_follows["session-a"].reconnect_owner,
                Some(owner)
            );
        }

        let message = reconnect_message(&mut receiver, delay_ms).await;
        assert!(matches!(
            &message,
            HostMessage::ReconnectDue { subscription_id, session_id, attempt: due }
                if *subscription_id == owner && session_id == "session-a" && *due == attempt
        ));
        handle_host(message, &mut state);
        let background = state.background_follows.get_mut("session-a").unwrap();
        owner = background.follow_owner.expect("new background owner");
        background.follow.as_ref().unwrap().abort();
        background.follow = Some(pending_task());
    }

    state.background_follows["session-a"]
        .follow
        .as_ref()
        .unwrap()
        .abort();
    handle_host(
        HostMessage::FollowEnded {
            subscription_id: owner,
            session_id: "session-a".into(),
            code: HostClientErrorCode::TransportFailure,
        },
        &mut state,
    );
    assert_eq!(state.background_follows["session-a"].attempt, 5);
    assert!(state.background_follows["session-a"].reconnect.is_none());
    assert_no_sixth_message(&mut receiver).await;
    state.stop_tasks();
}

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn tracked_task(dropped: Arc<AtomicBool>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _flag = DropFlag(dropped);
        future::pending::<()>().await;
    })
}

#[tokio::test]
async fn fifth_background_follow_aborts_oldest_tasks_and_preserves_remaining_order() {
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    let oldest_follow_dropped = Arc::new(AtomicBool::new(false));
    let oldest_reconnect_dropped = Arc::new(AtomicBool::new(false));

    state.add_background_follow_for_test(
        "session-1".into(),
        1,
        SubscriptionId::new(1),
        tracked_task(oldest_follow_dropped.clone()),
    );
    state
        .background_follows
        .get_mut("session-1")
        .unwrap()
        .reconnect = Some(tracked_task(oldest_reconnect_dropped.clone()));
    state
        .background_follows
        .get_mut("session-1")
        .unwrap()
        .reconnect_owner = Some(SubscriptionId::new(1));
    for value in 2..=4 {
        state.add_background_follow_for_test(
            format!("session-{value}"),
            value,
            SubscriptionId::new(value),
            pending_task(),
        );
    }
    tokio::task::yield_now().await;
    state.add_background_follow_for_test(
        "session-5".into(),
        5,
        SubscriptionId::new(5),
        pending_task(),
    );
    tokio::task::yield_now().await;

    assert!(oldest_follow_dropped.load(Ordering::SeqCst));
    assert!(oldest_reconnect_dropped.load(Ordering::SeqCst));
    assert_eq!(
        state.background_follow_order(),
        ["session-2", "session-3", "session-4", "session-5"]
    );
    for value in 2..=5 {
        let follow = &state.background_follows[&format!("session-{value}")];
        assert_eq!(follow.observed_position, value);
        assert_eq!(follow.follow_owner, Some(SubscriptionId::new(value)));
        assert!(follow.follow.is_some());
    }
    state.stop_tasks();
}
