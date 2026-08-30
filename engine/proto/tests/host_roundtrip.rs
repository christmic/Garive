use garive_proto::com::garive::host::v1::{
    ContinueTurnRequestV1, CreateSessionRequestV1, HostEventV1, TurnCommandResponseV1,
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
