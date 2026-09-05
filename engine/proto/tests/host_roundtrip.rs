use garive_proto::com::garive::host::v1::{
    turn_event_request_v1, AgentDefinitionPageV1, AgentDefinitionSummaryV1, ApprovalDecisionV1,
    ContinueTurnRequestV1, CreateSessionRequestV1, HostActivityV1, HostEventV1, SessionPageV1,
    SessionSummaryV1, SessionViewV1, SuspensionViewV1, TurnApprovalEventV1, TurnAskReplyEventV1,
    TurnCommandResponseV1, TurnEventRequestV1, TurnExternalInputEventV1, TurnSteerEventV1,
    TurnTimelineItemV1, TurnTimelinePageV1,
};
use prost::Message;

#[test]
fn generated_host_v1_round_trips_live_commands_events_and_responses() {
    let request = CreateSessionRequestV1 {
        agent_id: "agent-main".into(),
    };
    assert_eq!(
        CreateSessionRequestV1::decode(request.encode_to_vec().as_slice()).unwrap(),
        request
    );
    let event = HostEventV1 {
        api_version: "v1".into(),
        session_id: "session-1".into(),
        position: u64::MAX,
        event: "future.unknown".into(),
        turn_id: String::new(),
        execution_id: String::new(),
        text: String::new(),
        activity: Some(HostActivityV1 {
            api_version: "v1".into(),
            activity_id: "activity-1".into(),
            kind: "future_kind".into(),
            label_key: "agent.activity.unknown".into(),
            state: "future_state".into(),
            source_position: u64::MAX,
            terminal: false,
            safe_code: Some("future_code".into()),
        }),
    };
    assert_eq!(
        HostEventV1::decode(event.encode_to_vec().as_slice()).unwrap(),
        event
    );
    let continuation = ContinueTurnRequestV1 {
        session_id: "session-1".into(),
        suspension_id: "suspension-1".into(),
        expected_session_version: 3,
        input: String::new(),
        input_json: Some("true".into()),
    };
    assert_eq!(
        ContinueTurnRequestV1::decode(continuation.encode_to_vec().as_slice()).unwrap(),
        continuation
    );
    let response = TurnCommandResponseV1 {
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        execution_id: "execution-1".into(),
        committed_position: u64::MAX,
    };
    assert_eq!(
        TurnCommandResponseV1::decode(response.encode_to_vec().as_slice()).unwrap(),
        response
    );
}

fn varint(bytes: &[u8], cursor: &mut usize) -> u64 {
    let mut output = 0;
    for shift in (0..64).step_by(7) {
        let byte = bytes[*cursor];
        *cursor += 1;
        output |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return output;
        }
    }
    panic!("invalid protobuf varint")
}

fn top_level_tags<M: Message>(value: &M) -> Vec<u32> {
    let bytes = value.encode_to_vec();
    let mut cursor = 0;
    let mut tags = Vec::new();
    while cursor < bytes.len() {
        let key = varint(&bytes, &mut cursor);
        tags.push((key >> 3) as u32);
        match key & 7 {
            0 => _ = varint(&bytes, &mut cursor),
            1 => cursor += 8,
            2 => cursor += varint(&bytes, &mut cursor) as usize,
            5 => cursor += 4,
            wire => panic!("unsupported protobuf wire type {wire}"),
        }
    }
    tags
}

#[test]
fn host_read_and_activity_tag_allocation_is_exact() {
    let definition = AgentDefinitionSummaryV1 {
        api_version: "v1".into(),
        definition_id: "d".into(),
        definition_revision: "r".into(),
        capabilities: vec!["c".into()],
    };
    assert_eq!(top_level_tags(&definition), [1, 2, 3, 4]);
    assert_eq!(
        top_level_tags(&AgentDefinitionPageV1 {
            api_version: "v1".into(),
            definitions: vec![definition],
        }),
        [1, 2]
    );
    let session = SessionSummaryV1 {
        agent_id: "agent-main".into(),
        api_version: "v1".into(),
        session_id: "s".into(),
        agent_instance_id: "a".into(),
        definition_id: "d".into(),
        definition_revision: "r".into(),
        opened_at: "t".into(),
        latest_position: 1,
        latest_turn_id: Some("t".into()),
        latest_turn_state: Some("future".into()),
        turn_count: 1,
    };
    assert_eq!(top_level_tags(&session), (1..=11).collect::<Vec<_>>());
    assert_eq!(
        top_level_tags(&SessionPageV1 {
            api_version: "v1".into(),
            sessions: vec![session.clone()],
            next_before: Some("c".into()),
        }),
        [1, 2, 3]
    );
    assert_eq!(
        top_level_tags(&SessionViewV1 {
            api_version: "v1".into(),
            session: Some(session),
            observed_max_position: 1,
        }),
        [1, 2, 3]
    );
    let suspension = SuspensionViewV1 {
        suspension_id: "s".into(),
        session_version: 1,
        kind: "future".into(),
        prompt_schema: "p".into(),
        prompt_json: vec![1],
        prompt_digest: "d".into(),
        response_schema_json: Some(vec![1]),
        response_schema_digest: Some("d".into()),
    };
    assert_eq!(top_level_tags(&suspension), (1..=8).collect::<Vec<_>>());
    let activity = HostActivityV1 {
        api_version: "v1".into(),
        activity_id: "a".into(),
        kind: "future".into(),
        label_key: "l".into(),
        state: "future".into(),
        source_position: 1,
        terminal: true,
        safe_code: Some("future".into()),
    };
    assert_eq!(top_level_tags(&activity), (1..=8).collect::<Vec<_>>());
    assert_eq!(
        top_level_tags(&TurnTimelineItemV1 {
            turn_id: "t".into(),
            started_position: 1,
            latest_position: 1,
            state: "future".into(),
            user_text: "u".into(),
            completion_text: Some("c".into()),
            suspension: Some(suspension),
            content_truncated: true,
            activities: vec![activity.clone()],
        }),
        (1..=9).collect::<Vec<_>>()
    );
    assert_eq!(
        top_level_tags(&TurnTimelinePageV1 {
            api_version: "v1".into(),
            session_id: "s".into(),
            items: vec![TurnTimelineItemV1::default()],
            scanned_through_position: 1,
            observed_max_position: 1,
            has_more: true,
        }),
        (1..=6).collect::<Vec<_>>()
    );
    assert_eq!(
        top_level_tags(&HostEventV1 {
            api_version: "v1".into(),
            session_id: "s".into(),
            position: 1,
            event: "future".into(),
            turn_id: "t".into(),
            execution_id: "e".into(),
            text: "x".into(),
            activity: Some(activity),
        }),
        (1..=8).collect::<Vec<_>>()
    );
}

#[test]
fn generated_host_v1_round_trips_read_model_presence_and_max_positions() {
    let definition = AgentDefinitionSummaryV1 {
        api_version: "v1".into(),
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        capabilities: vec!["chat".into(), "tools".into()],
    };
    let definitions = AgentDefinitionPageV1 {
        api_version: "v1".into(),
        definitions: vec![definition],
    };
    assert_eq!(
        AgentDefinitionPageV1::decode(definitions.encode_to_vec().as_slice()).unwrap(),
        definitions
    );

    let session = SessionSummaryV1 {
        agent_id: "agent-main".into(),
        api_version: "v1".into(),
        session_id: "session-1".into(),
        agent_instance_id: "agent-1".into(),
        definition_id: "definition-main".into(),
        definition_revision: "revision-7".into(),
        opened_at: "2026-08-30T00:00:00Z".into(),
        latest_position: u64::MAX,
        latest_turn_id: Some("turn-1".into()),
        latest_turn_state: Some("suspended".into()),
        turn_count: 1,
    };
    let sessions = SessionPageV1 {
        api_version: "v1".into(),
        sessions: vec![session],
        next_before: Some("opaque".into()),
    };
    assert_eq!(
        SessionPageV1::decode(sessions.encode_to_vec().as_slice()).unwrap(),
        sessions
    );

    let timeline = TurnTimelinePageV1 {
        api_version: "v1".into(),
        session_id: "session-1".into(),
        items: vec![TurnTimelineItemV1 {
            turn_id: "turn-1".into(),
            started_position: 2,
            latest_position: u64::MAX,
            state: "suspended".into(),
            user_text: "Draft the release plan".into(),
            completion_text: None,
            suspension: Some(SuspensionViewV1 {
                suspension_id: "suspension-1".into(),
                session_version: 3,
                kind: "approval".into(),
                prompt_schema: "garive.public-suspension-prompt.v1".into(),
                prompt_json: br#"{"schema_version":1}"#.to_vec(),
                prompt_digest: "digest".into(),
                response_schema_json: Some(br#"{"type":"boolean"}"#.to_vec()),
                response_schema_digest: Some("schema-digest".into()),
            }),
            content_truncated: false,
            activities: vec![HostActivityV1 {
                api_version: "v1".into(),
                activity_id: "activity-1".into(),
                kind: "tool".into(),
                label_key: "agent.activity.write_file".into(),
                state: "attention_required".into(),
                source_position: u64::MAX - 1,
                terminal: false,
                safe_code: Some("receipt_missing".into()),
            }],
        }],
        scanned_through_position: u64::MAX,
        observed_max_position: u64::MAX,
        has_more: false,
    };
    assert_eq!(
        TurnTimelinePageV1::decode(timeline.encode_to_vec().as_slice()).unwrap(),
        timeline
    );
}

#[test]
fn turn_event_request_v1_envelope_round_trips_for_every_kind() {
    let steer = TurnEventRequestV1 {
        body: Some(turn_event_request_v1::Body::Steer(TurnSteerEventV1 {
            session_id: "session-1".into(),
            text: "trust me".into(),
        })),
    };
    assert_eq!(
        TurnEventRequestV1::decode(steer.encode_to_vec().as_slice()).unwrap(),
        steer
    );

    let approval = TurnEventRequestV1 {
        body: Some(turn_event_request_v1::Body::Approval(TurnApprovalEventV1 {
            session_id: "session-1".into(),
            suspension_id: "suspension-1".into(),
            expected_session_version: 5,
            decision: ApprovalDecisionV1::Approve as i32,
        })),
    };
    assert_eq!(
        TurnEventRequestV1::decode(approval.encode_to_vec().as_slice()).unwrap(),
        approval
    );

    let deny = TurnEventRequestV1 {
        body: Some(turn_event_request_v1::Body::Approval(TurnApprovalEventV1 {
            session_id: "session-1".into(),
            suspension_id: "suspension-2".into(),
            expected_session_version: 6,
            decision: ApprovalDecisionV1::Deny as i32,
        })),
    };
    assert_ne!(
        deny.encode_to_vec(),
        approval.encode_to_vec(),
        "approve and deny must encode to distinct bytes (decision is part of the replay tuple)"
    );
    assert_eq!(
        TurnEventRequestV1::decode(deny.encode_to_vec().as_slice()).unwrap(),
        deny
    );

    let ask_reply = TurnEventRequestV1 {
        body: Some(turn_event_request_v1::Body::AskReply(TurnAskReplyEventV1 {
            session_id: "session-1".into(),
            suspension_id: "suspension-3".into(),
            expected_session_version: 7,
            input_json: r#"{"answer":"yes"}"#.to_string(),
        })),
    };
    assert_eq!(
        TurnEventRequestV1::decode(ask_reply.encode_to_vec().as_slice()).unwrap(),
        ask_reply
    );

    let external_input = TurnEventRequestV1 {
        body: Some(turn_event_request_v1::Body::ExternalInput(
            TurnExternalInputEventV1 {
                session_id: "session-1".into(),
                suspension_id: "suspension-4".into(),
                expected_session_version: 8,
                text: "plain-text continuation".into(),
            },
        )),
    };
    assert_eq!(
        TurnEventRequestV1::decode(external_input.encode_to_vec().as_slice()).unwrap(),
        external_input
    );

    let empty = TurnEventRequestV1 { body: None };
    assert_eq!(
        TurnEventRequestV1::decode(empty.encode_to_vec().as_slice()).unwrap(),
        empty
    );
}

#[test]
fn turn_event_request_v1_tag_allocation_is_exact() {
    // Each top-level field-tag on TurnEventRequestV1 is one of {1,2,3,4}
    // (the `oneof` body discriminator). Rust-generated `body` uses a stable
    // tag per kind; assert each kind emits exactly its assigned tag.
    fn single_top_tag(req: &TurnEventRequestV1) -> Vec<u32> {
        top_level_tags(req)
    }
    assert_eq!(
        single_top_tag(&TurnEventRequestV1 {
            body: Some(turn_event_request_v1::Body::Steer(TurnSteerEventV1::default())),
        }),
        vec![1]
    );
    assert_eq!(
        single_top_tag(&TurnEventRequestV1 {
            body: Some(turn_event_request_v1::Body::Approval(TurnApprovalEventV1::default())),
        }),
        vec![2]
    );
    assert_eq!(
        single_top_tag(&TurnEventRequestV1 {
            body: Some(turn_event_request_v1::Body::AskReply(TurnAskReplyEventV1::default())),
        }),
        vec![3]
    );
    assert_eq!(
        single_top_tag(&TurnEventRequestV1 {
            body: Some(turn_event_request_v1::Body::ExternalInput(
                TurnExternalInputEventV1::default()
            )),
        }),
        vec![4]
    );

    // Inner shapes: TurnSteerEventV1 = {1,2}; TurnApprovalEventV1 = {1,2,3,4};
    // TurnAskReplyEventV1 = {1,2,3,4}; TurnExternalInputEventV1 = {1,2,3,4}.
    // Default values are NOT emitted; populate each field so the tag map is
    // exact and the proto3 wire contract is locked in.
    fn inner_tags<M: Message>(value: &M) -> Vec<u32> {
        top_level_tags(value)
    }
    assert_eq!(
        inner_tags(&TurnSteerEventV1 {
            session_id: "s".into(),
            text: "t".into(),
        }),
        vec![1, 2]
    );
    assert_eq!(
        inner_tags(&TurnApprovalEventV1 {
            session_id: "s".into(),
            suspension_id: "u".into(),
            expected_session_version: 1,
            decision: ApprovalDecisionV1::Approve as i32,
        }),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        inner_tags(&TurnAskReplyEventV1 {
            session_id: "s".into(),
            suspension_id: "u".into(),
            expected_session_version: 1,
            input_json: "{}".into(),
        }),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        inner_tags(&TurnExternalInputEventV1 {
            session_id: "s".into(),
            suspension_id: "u".into(),
            expected_session_version: 1,
            text: "t".into(),
        }),
        vec![1, 2, 3, 4]
    );
}
