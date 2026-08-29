use garive_adapter_anthropic_messages::{MessagesAdapterError, SseDecoder};

#[test]
fn every_byte_split_preserves_utf8_multiline_frame() {
    let bytes = "event: content_block_delta\r\nid: 7\r\nretry: 25\r\n\
                 data: {\"type\":\"content_block_delta\",\r\n\
                 data: \"delta\":{\"type\":\"text_delta\",\"text\":\"你好\"}}\r\n\r\n"
        .as_bytes();
    for split in 0..=bytes.len() {
        let mut decoder = SseDecoder::new();
        let mut frames = decoder.push(&bytes[..split]).unwrap();
        frames.extend(decoder.push(&bytes[split..]).unwrap());
        decoder.finish().unwrap();
        assert_eq!(frames.len(), 1, "split {split}");
        assert_eq!(frames[0].event(), Some("content_block_delta"));
        assert_eq!(frames[0].id(), Some("7"));
        assert_eq!(frames[0].retry(), Some(25));
        assert!(frames[0].data().contains("你好"));
    }
}

#[test]
fn malformed_or_incomplete_frames_fail_closed() {
    let mut retry = SseDecoder::new();
    assert_eq!(
        retry.push(b"retry: later\ndata: {}\n\n"),
        Err(MessagesAdapterError::InvalidSse)
    );
    let mut utf8 = SseDecoder::new();
    assert_eq!(
        utf8.push(b"data: \xff\n\n"),
        Err(MessagesAdapterError::InvalidSse)
    );
    let mut truncated = SseDecoder::new();
    assert!(truncated.push(b"data: partial").unwrap().is_empty());
    assert_eq!(
        truncated.finish(),
        Err(MessagesAdapterError::TruncatedStream)
    );
}
