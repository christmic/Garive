use garive_adapter_browser_cdp::{parse_incoming, CdpCommand, CdpIncoming, CdpProtocolError};
use serde_json::json;

#[test]
fn command_allows_only_the_frozen_method_subset() {
    assert!(CdpCommand::new(
        1,
        "Accessibility.getFullAXTree",
        json!({"depth":64,"frameId":"frame-1"}),
        Some("target-session-1".into())
    )
    .is_ok());
    assert_eq!(
        CdpCommand::new(2, "Runtime.evaluate", json!({}), None),
        Err(CdpProtocolError::InvalidMessage)
    );
}

#[test]
fn incoming_result_error_and_event_keep_flat_session_binding() {
    let result = parse_incoming(
        br#"{"id":7,"result":{"protocolVersion":"1.3"},"sessionId":"s-1"}"#,
        1_024,
    )
    .expect("result");
    assert!(matches!(
        result,
        CdpIncoming::Result { id: 7, session_id: Some(session), .. } if session == "s-1"
    ));
    let error = parse_incoming(
        br#"{"id":8,"error":{"code":-32601,"message":"method missing"}}"#,
        1_024,
    )
    .expect("error");
    assert!(matches!(error, CdpIncoming::Error { id: 8, .. }));
    let event = parse_incoming(
        br#"{"method":"Page.loadEventFired","params":{"timestamp":1},"sessionId":"s-1"}"#,
        1_024,
    )
    .expect("event");
    assert!(matches!(event, CdpIncoming::Event { .. }));
}

#[test]
fn frames_and_mixed_terminals_fail_closed() {
    assert_eq!(
        parse_incoming(b"{}", 1),
        Err(CdpProtocolError::FrameBoundExceeded)
    );
    assert_eq!(
        parse_incoming(
            br#"{"id":1,"result":{},"error":{"code":1,"message":"x"}}"#,
            1_024
        ),
        Err(CdpProtocolError::InvalidMessage)
    );
    assert_eq!(
        parse_incoming(br#"{"id":1,"id":2,"result":{}}"#, 1_024),
        Err(CdpProtocolError::InvalidJson)
    );
}
