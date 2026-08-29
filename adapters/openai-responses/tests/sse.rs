use garive_adapter_openai_responses::{ResponsesAdapterError, SseDecoder};

#[test]
fn every_byte_split_preserves_utf8_multiline_frame() {
    let bytes = "event: response.output_text.delta\r\nid: 7\r\nretry: 25\r\n\
                 data: {\"type\":\"response.output_text.delta\",\r\n\
                 data: \"delta\":\"你好\"}\r\n\r\n"
        .as_bytes();
    for split in 0..=bytes.len() {
        let mut decoder = SseDecoder::new();
        let mut frames = decoder.push(&bytes[..split]).unwrap();
        frames.extend(decoder.push(&bytes[split..]).unwrap());
        decoder.finish().unwrap();
        assert_eq!(frames.len(), 1, "split {split}");
        assert_eq!(frames[0].event(), Some("response.output_text.delta"));
        assert_eq!(frames[0].id(), Some("7"));
        assert_eq!(frames[0].retry(), Some(25));
        assert!(frames[0].data().contains("你好"));
        assert!(frames[0].data().contains('\n'));
    }
}

#[test]
fn comments_and_unknown_fields_do_not_create_events() {
    let mut decoder = SseDecoder::new();
    let frames = decoder
        .push(b": keepalive\nunknown: value\n\ndata: one\ndata: two\n\n")
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].data(), "one\ntwo");
    decoder.finish().unwrap();
}

#[test]
fn malformed_retry_utf8_and_incomplete_eof_fail_closed() {
    let mut retry = SseDecoder::new();
    assert_eq!(
        retry.push(b"retry: later\ndata: {}\n\n"),
        Err(ResponsesAdapterError::InvalidSse)
    );
    let mut utf8 = SseDecoder::new();
    assert_eq!(
        utf8.push(b"data: \xff\n\n"),
        Err(ResponsesAdapterError::InvalidSse)
    );
    let mut truncated = SseDecoder::new();
    assert!(truncated.push(b"data: partial").unwrap().is_empty());
    assert_eq!(
        truncated.finish(),
        Err(ResponsesAdapterError::TruncatedStream)
    );
}
