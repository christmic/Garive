use garive_proto::com::garive::host::v1::{
    AgentDefinitionPageV1, AgentDefinitionSummaryV1, ContinueTurnRequestV1, CreateSessionRequestV1,
    HostActivityV1, HostEventV1, SessionPageV1, SessionSummaryV1, SuspensionViewV1,
    TurnCommandResponseV1, TurnTimelineItemV1, TurnTimelinePageV1,
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
