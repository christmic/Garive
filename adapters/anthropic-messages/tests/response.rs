use garive_adapter_anthropic_messages::{
    DecodedResponse, Header, MessagesAdapter, MessagesAdapterConfig, OutputBlock, StopReason,
};
use serde_json::{json, Value};

fn adapter() -> MessagesAdapter {
    MessagesAdapter::new(
        MessagesAdapterConfig::new(
            "https://example.test/messages",
            vec![],
            "x-version",
            "fixture",
        )
        .unwrap(),
    )
}

#[test]
fn official_message_shape_decodes_without_usage_normalization() {
    let body = include_bytes!("../../../spec/fixtures/providers/anthropic/messages/ordinary.json");
    let decoded = adapter().decode_response(200, &[], body).unwrap();
    let DecodedResponse::Message {
        status, message, ..
    } = decoded
    else {
        panic!("expected message")
    };
    assert_eq!(status, 200);
    assert_eq!(message.stop_reason, Some(StopReason::ToolUse));
    assert_eq!(message.usage.input_tokens, 8);
    assert_eq!(message.usage.cache_creation_input_tokens, Some(2));
    assert!(matches!(message.content[0], OutputBlock::Text(_)));
    assert!(matches!(message.content[1], OutputBlock::ToolUse(_)));
}

#[test]
fn hosted_output_block_is_a_lossless_extension() {
    let mut value: Value = serde_json::from_slice(include_bytes!(
        "../../../spec/fixtures/providers/anthropic/messages/ordinary.json"
    ))
    .unwrap();
    let hosted = json!({"type":"web_search_tool_result","tool_use_id":"srv_1","content":[{"url":"https://example.test"}]});
    value["content"]
        .as_array_mut()
        .unwrap()
        .push(hosted.clone());
    let bytes = serde_json::to_vec(&value).unwrap();
    let DecodedResponse::Message { message, .. } =
        adapter().decode_response(200, &[], &bytes).unwrap()
    else {
        panic!("expected message")
    };
    let OutputBlock::Extension(extension) = &message.content[2] else {
        panic!("hosted block must not become a client-tool block")
    };
    assert_eq!(Value::Object(extension.clone()), hosted);
}

#[test]
fn error_status_and_open_type_remain_protocol_facts() {
    let headers =
        vec![Header::new("content-type", "application/json; charset=utf-8", false).unwrap()];
    let body = br#"{"type":"error","error":{"type":"future_capacity_error","message":"later"},"request_id":"req_1"}"#;
    let DecodedResponse::Error { status, error, .. } =
        adapter().decode_response(599, &headers, body).unwrap()
    else {
        panic!("expected error")
    };
    assert_eq!(status, 599);
    assert_eq!(error.error.r#type, "future_capacity_error");
}

#[test]
fn incompatible_media_and_malformed_known_blocks_fail() {
    let headers = vec![Header::new("content-type", "text/plain", false).unwrap()];
    assert!(adapter().decode_response(200, &headers, b"{}").is_err());

    let mut value: Value = serde_json::from_slice(include_bytes!(
        "../../../spec/fixtures/providers/anthropic/messages/ordinary.json"
    ))
    .unwrap();
    value["content"] = json!([{"type":"text","text":""}]);
    assert!(adapter()
        .decode_response(200, &[], &serde_json::to_vec(&value).unwrap())
        .is_err());
}
