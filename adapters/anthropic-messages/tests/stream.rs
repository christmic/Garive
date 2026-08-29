use garive_adapter_anthropic_messages::{
    DeltaKind, MessagesAdapterError, MessagesStreamDecoder, StreamEventKind,
};

fn decode(bytes: &[u8]) -> Result<Vec<StreamEventKind>, MessagesAdapterError> {
    let mut decoder = MessagesStreamDecoder::new();
    let events = decoder.push(bytes)?;
    decoder.finish()?;
    Ok(events
        .into_iter()
        .map(|event| event.kind().clone())
        .collect())
}

#[test]
fn official_complete_fixture_has_typed_lifecycle() {
    let bytes = include_bytes!("../../../spec/fixtures/protocols/anthropic-messages/complete.sse");
    let events = decode(bytes).unwrap();
    assert_eq!(events.first(), Some(&StreamEventKind::MessageStart));
    assert!(events.contains(&StreamEventKind::ContentBlockDelta(DeltaKind::Text)));
    assert!(events.contains(&StreamEventKind::ContentBlockDelta(DeltaKind::InputJson)));
    assert_eq!(events.last(), Some(&StreamEventKind::MessageStop));
}

#[test]
fn every_fixture_byte_split_is_incremental_and_equivalent() {
    let bytes = include_bytes!("../../../spec/fixtures/protocols/anthropic-messages/complete.sse");
    let expected = decode(bytes).unwrap();
    for split in 0..=bytes.len() {
        let mut decoder = MessagesStreamDecoder::new();
        let mut events = decoder.push(&bytes[..split]).unwrap();
        events.extend(decoder.push(&bytes[split..]).unwrap());
        decoder.finish().unwrap();
        assert_eq!(
            events
                .into_iter()
                .map(|event| event.kind().clone())
                .collect::<Vec<_>>(),
            expected,
            "split {split}"
        );
    }
}

#[test]
fn thinking_signature_and_redaction_remain_distinct() {
    let events = decode(include_bytes!(
        "../../../spec/fixtures/protocols/anthropic-messages/thinking.sse"
    ))
    .unwrap();
    assert!(events.contains(&StreamEventKind::ContentBlockDelta(DeltaKind::Thinking)));
    assert!(events.contains(&StreamEventKind::ContentBlockDelta(DeltaKind::Signature)));
}

#[test]
fn protocol_error_is_terminal_without_retry_classification() {
    let events = decode(include_bytes!(
        "../../../spec/fixtures/protocols/anthropic-messages/stream-error.sse"
    ))
    .unwrap();
    assert_eq!(events.last(), Some(&StreamEventKind::Error));
}

#[test]
fn event_name_lifecycle_and_tool_json_fail_closed() {
    let mismatch = b"event: ping\ndata: {\"type\":\"message_stop\"}\n\n";
    let mut decoder = MessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(mismatch),
        Err(MessagesAdapterError::InvalidSse)
    );

    let before_start = b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let mut decoder = MessagesStreamDecoder::new();
    assert!(matches!(
        decoder.push(before_start),
        Err(MessagesAdapterError::InvalidLifecycle(_))
    ));

    let invalid_tool = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n";
    let mut decoder = MessagesStreamDecoder::new();
    assert_eq!(
        decoder.push(invalid_tool),
        Err(MessagesAdapterError::InvalidJson)
    );
}

#[test]
fn missing_terminal_is_truncated() {
    let bytes = include_bytes!("../../../spec/fixtures/protocols/anthropic-messages/truncated.sse");
    let mut decoder = MessagesStreamDecoder::new();
    let _ = decoder.push(bytes);
    assert_eq!(decoder.finish(), Err(MessagesAdapterError::TruncatedStream));
}
