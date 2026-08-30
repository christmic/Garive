use garive_proto::com::garive::host::v1::{
    ContinueTurnRequestV1, CreateSessionRequestV1, HostActivityV1, HostEventV1, SessionSummaryV1,
    SuspensionViewV1, TurnCommandResponseV1, TurnTimelineItemV1, TurnTimelinePageV1,
};
use prost::Message;

#[test]
fn generated_host_v1_round_trips_live_commands_events_and_responses() {
    let request = CreateSessionRequestV1 {
        agent_definition_id: "definition-main".into(),
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
            activity_id: "activity".into(),
            kind: "future-kind".into(),
            label_key: "agent.activity.unknown".into(),
            state: "future-state".into(),
            source_position: u64::MAX,
            terminal: false,
            safe_code: None,
        }),
    };
    assert_eq!(
        HostEventV1::decode(event.encode_to_vec().as_slice()).unwrap(),
        event
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

#[test]
fn typed_continuation_json_uses_the_additive_tag() {
    let request = ContinueTurnRequestV1 {
        session_id: "session".into(),
        suspension_id: "suspension".into(),
        expected_session_version: 7,
        input: String::new(),
        input_json: Some(r#"{"approved":true}"#.into()),
    };
    assert_eq!(
        ContinueTurnRequestV1::decode(request.encode_to_vec().as_slice()).unwrap(),
        request
    );
}

#[test]
fn h2_timeline_preserves_presence_bytes_and_unsigned_positions() {
    let page = TurnTimelinePageV1 {
        api_version: "v1".into(),
        session_id: "session".into(),
        items: vec![TurnTimelineItemV1 {
            turn_id: "turn".into(),
            started_position: 2,
            latest_position: u64::MAX,
            state: "suspended".into(),
            user_text: "hello".into(),
            completion_text: None,
            suspension: Some(SuspensionViewV1 {
                suspension_id: "suspension".into(),
                session_version: 9,
                kind: "approval_required".into(),
                prompt_schema: "garive.public-suspension-prompt.v1".into(),
                prompt_json: br#"{"schema_version":1}"#.to_vec(),
                prompt_digest: "digest".into(),
                response_schema_json: Some(br#"{"type":"boolean"}"#.to_vec()),
                response_schema_digest: Some("schema-digest".into()),
            }),
            content_truncated: false,
            activities: Vec::new(),
        }],
        scanned_through_position: u64::MAX,
        observed_max_position: u64::MAX,
        has_more: false,
    };
    assert_eq!(
        TurnTimelinePageV1::decode(page.encode_to_vec().as_slice()).unwrap(),
        page
    );

    let summary = SessionSummaryV1 {
        api_version: "v1".into(),
        session_id: "session".into(),
        agent_instance_id: "agent".into(),
        definition_id: "definition".into(),
        definition_revision: "revision".into(),
        opened_at: "2026-01-01T00:00:00Z".into(),
        latest_position: 3,
        latest_turn_id: Some("turn".into()),
        latest_turn_state: None,
        turn_count: 1,
    };
    assert_eq!(
        SessionSummaryV1::decode(summary.encode_to_vec().as_slice()).unwrap(),
        summary
    );
}
