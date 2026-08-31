use super::*;
use garive_host_client::{HostActivity, LiveOutputEndReason, LiveOutputEventKind, SuspensionView};

const CANARY: &str = "host-message-private-content-canary";

fn session_summary() -> SessionSummary {
    SessionSummary {
        api_version: CANARY.into(),
        session_id: CANARY.into(),
        agent_instance_id: CANARY.into(),
        definition_id: CANARY.into(),
        definition_revision: CANARY.into(),
        opened_at: CANARY.into(),
        latest_position: 41,
        latest_turn_id: Some(CANARY.into()),
        latest_turn_state: Some(CANARY.into()),
        turn_count: 2,
    }
}

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

fn timeline_item() -> TurnTimelineItem {
    TurnTimelineItem {
        turn_id: CANARY.into(),
        started_position: 43,
        latest_position: 44,
        state: CANARY.into(),
        user_text: CANARY.into(),
        completion_text: Some(CANARY.into()),
        suspension: Some(SuspensionView {
            suspension_id: CANARY.into(),
            session_version: 45,
            kind: CANARY.into(),
            prompt_schema: CANARY.into(),
            prompt_json: CANARY.into(),
            prompt_digest: CANARY.into(),
            response_schema_json: Some(CANARY.into()),
            response_schema_digest: Some(CANARY.into()),
        }),
        content_truncated: false,
        activities: vec![activity()],
    }
}

fn live_output(kind: LiveOutputEventKind) -> HostMessage {
    HostMessage::LiveOutput(LiveOutputEvent {
        api_version: CANARY.into(),
        session_id: CANARY.into(),
        turn_id: CANARY.into(),
        execution_id: CANARY.into(),
        stream_id: CANARY.into(),
        sequence: 46,
        kind,
    })
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
        HostMessage::Bootstrapped {
            definitions: vec![AgentDefinitionSummary {
                api_version: CANARY.into(),
                definition_id: CANARY.into(),
                definition_revision: CANARY.into(),
                capabilities: vec![CANARY.into()],
            }],
            sessions: vec![session_summary()],
            next_before: Some(CANARY.into()),
        },
        "Bootstrapped",
        &[
            "definition_count: 1",
            "session_count: 1",
            "has_next_page: true",
        ],
    );
    assert_safe(
        HostMessage::SnapshotLoaded {
            request_id: 47,
            session_id: CANARY.into(),
            view: SessionView {
                api_version: CANARY.into(),
                session: session_summary(),
                observed_max_position: 48,
            },
            items: vec![timeline_item()],
            follow_position: 49,
        },
        "SnapshotLoaded",
        &["request_id: 47", "item_count: 1", "follow_position: 49"],
    );
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
        HostMessage::Event(HostEvent {
            api_version: CANARY.into(),
            session_id: CANARY.into(),
            position: 52,
            event: CANARY.into(),
            turn_id: CANARY.into(),
            execution_id: CANARY.into(),
            text: CANARY.into(),
            activity: Some(activity()),
        }),
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
            session_id: CANARY.into(),
            code: HostClientErrorCode::InvalidEvent,
        },
        "FollowEnded",
        &["code: \"invalid_event\""],
    );
    assert_safe(
        HostMessage::LiveFollowEnded {
            session_id: CANARY.into(),
            code: HostClientErrorCode::TransportFailure,
        },
        "LiveFollowEnded",
        &["code: \"transport_failure\""],
    );
    assert_safe(
        HostMessage::ReconnectDue {
            session_id: CANARY.into(),
            attempt: 4,
        },
        "ReconnectDue",
        &["attempt: 4"],
    );
    assert_safe(
        HostMessage::LiveReconnectDue {
            session_id: CANARY.into(),
            attempt: 5,
        },
        "LiveReconnectDue",
        &["attempt: 5"],
    );

    for operation in [
        HostOperation::Bootstrap,
        HostOperation::Snapshot { request_id: 54 },
        HostOperation::Mutation {
            command_id: CANARY.into(),
        },
    ] {
        assert_safe(
            HostMessage::Failed {
                operation,
                error: garive_host_client::HostClientError {
                    code: HostClientErrorCode::HostFailure,
                    status: Some(503),
                },
            },
            "Failed",
            &["code: \"host_failure\""],
        );
    }
}
