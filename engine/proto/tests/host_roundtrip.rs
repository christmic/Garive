use garive_proto::com::garive::host::v1::{FakeHostCommandV1, FakeHostScenarioV1, HostEventV1};
use prost::Message;

#[test]
fn generated_host_v1_round_trips_unknown_event_text_and_u64_position() {
    let scenario = FakeHostScenarioV1 {
        api_version: "garive.host.v1".into(),
        command: Some(FakeHostCommandV1 {
            agent_definition_id: "garive.default".into(),
            text: "hello".into(),
        }),
        events: vec![HostEventV1 {
            api_version: "garive.host.v1".into(),
            session_id: "session-1".into(),
            position: u64::MAX,
            event: "future.unknown".into(),
            turn_id: String::new(),
            execution_id: String::new(),
            text: String::new(),
        }],
    };
    let decoded = FakeHostScenarioV1::decode(scenario.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded, scenario);
}
