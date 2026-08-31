use super::*;
use garive_host_client::{HostActivity, LiveOutputEndReason, LiveOutputEventKind};

const CANARY: &str = "host-message-private-content-canary";

fn activity() -> HostActivity {
    HostActivity {
        api_version: CANARY.into(),
        activity_id: CANARY.into(),
        kind: CANARY.into(),
        label_key: CANARY.into(),
        state: CANARY.into(),
        source_position: 42,
        terminal: false,
        safe_code: Some(CANARY.into()),
    }
}

fn live_output(kind: LiveOutputEventKind) -> HostMessage {
    HostMessage::LiveOutput {
        subscription_id: LiveSubscriptionId::new(2),
        event: LiveOutputEvent {
            api_version: CANARY.into(),
            session_id: CANARY.into(),
            turn_id: CANARY.into(),
            execution_id: CANARY.into(),
            stream_id: CANARY.into(),
            sequence: 46,
            kind,
        },
    }
}

#[test]
fn host_message_debug_is_content_safe_for_every_variant() {
    let assert_safe = |message: HostMessage, variant: &str, expected: &[&str]| {
        let debug = format!("{message:?}");
        assert!(debug.contains(variant), "missing variant in {debug}");
        assert!(
            !debug.contains(CANARY),
            "private content escaped through {debug}"
        );
        for fragment in expected {
            assert!(debug.contains(fragment), "missing {fragment:?} in {debug}");
        }
    };

    assert_safe(
        HostMessage::SessionCreated {
            command_id: CANARY.into(),
            response: CreateSessionResponse {
                session_id: CANARY.into(),
                agent_instance_id: CANARY.into(),
                committed_position: 50,
            },
        },
        "SessionCreated",
        &["committed_position: 50"],
    );
    assert_safe(
        HostMessage::TurnAccepted {
            command_id: CANARY.into(),
            session_id: CANARY.into(),
            submitted_text: CANARY.into(),
            response: TurnCommandResponse {
                session_id: CANARY.into(),
                turn_id: CANARY.into(),
                execution_id: CANARY.into(),
                committed_position: 51,
            },
        },
        "TurnAccepted",
        &["committed_position: 51"],
    );
    assert_safe(
        HostMessage::Event {
            subscription_id: SubscriptionId::new(1),
            event: HostEvent {
                api_version: CANARY.into(),
                session_id: CANARY.into(),
                position: 52,
                event: CANARY.into(),
                turn_id: CANARY.into(),
                execution_id: CANARY.into(),
                text: CANARY.into(),
                activity: Some(activity()),
            },
        },
        "Event",
        &["position: 52"],
    );
    assert_safe(
        live_output(LiveOutputEventKind::Snapshot {
            text: CANARY.into(),
            through_sequence: 53,
        }),
        "LiveOutput",
        &["sequence: 46", "kind: \"snapshot\""],
    );
    assert_safe(
        live_output(LiveOutputEventKind::TextDelta {
            text: CANARY.into(),
        }),
        "LiveOutput",
        &["kind: \"text_delta\""],
    );
    assert_safe(
        live_output(LiveOutputEventKind::PhaseChanged {
            phase: CANARY.into(),
            label_key: CANARY.into(),
        }),
        "LiveOutput",
        &["kind: \"phase_changed\""],
    );
    assert_safe(
        live_output(LiveOutputEventKind::PreviewUnavailable),
        "LiveOutput",
        &["kind: \"preview_unavailable\""],
    );
    assert_safe(
        live_output(LiveOutputEventKind::Ended {
            reason: LiveOutputEndReason::PublisherClosed,
        }),
        "LiveOutput",
        &["kind: \"ended\""],
    );
    assert_safe(
        HostMessage::FollowEnded {
            subscription_id: SubscriptionId::new(1),
            session_id: CANARY.into(),
            code: HostClientErrorCode::InvalidEvent,
        },
        "FollowEnded",
        &["code: \"invalid_event\""],
    );
    assert_safe(
        HostMessage::LiveFollowEnded {
            subscription_id: LiveSubscriptionId::new(2),
            session_id: CANARY.into(),
            code: HostClientErrorCode::TransportFailure,
        },
        "LiveFollowEnded",
        &["code: \"transport_failure\""],
    );
    assert_safe(
        HostMessage::ReconnectDue {
            subscription_id: SubscriptionId::new(1),
            session_id: CANARY.into(),
            attempt: 4,
        },
        "ReconnectDue",
        &["attempt: 4"],
    );
    assert_safe(
        HostMessage::LiveReconnectDue {
            subscription_id: LiveSubscriptionId::new(2),
            session_id: CANARY.into(),
            attempt: 5,
        },
        "LiveReconnectDue",
        &["attempt: 5"],
    );

    assert_safe(
        HostMessage::Failed {
            operation: HostOperation::Mutation {
                command_id: CANARY.into(),
            },
            error: garive_host_client::HostClientError {
                code: HostClientErrorCode::HostFailure,
                status: Some(503),
            },
        },
        "Failed",
        &["code: \"host_failure\""],
    );
}
