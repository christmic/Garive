use garive_llm::{
    InvokeOutcome, ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings,
    ModelRequest, ModelRequestId, ModelRole, ModelStopReason, ModelTargetId, TextMode, TokenCount,
    ToolDescriptor,
};
use garive_llm_anthropic::{parse_response, parse_sse, render_request, AnthropicAdapterError};
use serde_json::Value;
use std::{fs, path::PathBuf};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/fixtures/providers/anthropic/messages")
            .join(name),
    )
    .unwrap()
}

fn request() -> ModelRequest {
    ModelRequest { request_id: ModelRequestId::new("request-1"),
        target_id: ModelTargetId::new("claude-sonnet-4-5"),
        required_capabilities: vec![ModelCapability::Text, ModelCapability::Tools, ModelCapability::Streaming],
        input_items: vec![
            ModelInputItem::Message { role: ModelRole::System,
                content: vec![ModelInputContent::Text("be concise".into())] },
            ModelInputItem::Message { role: ModelRole::User,
                content: vec![ModelInputContent::Text("hello".into())] },
        ],
        tools: vec![ToolDescriptor { name: "weather".into(), description: "Lookup weather".into(),
            definition_revision: "1".into(), input_schema_json:
                r#"{"type":"object","properties":{"city":{"type":"string"}},"required":["city"],"additionalProperties":false}"#.into(),
            strict: false }],
        output: ModelOutputSettings { max_output_tokens: Some(128), text_mode: TextMode::Plain,
            reasoning_visibility: false },
        trace_metadata: vec![("user_id".into(), "fixture".into())] }
}

#[test]
fn request_matches_official_shape_fixture() {
    let expected: Value = serde_json::from_slice(&fixture("request.json")).unwrap();
    assert_eq!(render_request(&request(), true).unwrap(), expected);
}

#[test]
fn ordinary_and_stream_preserve_tool_and_cache_usage() {
    let InvokeOutcome::Completed {
        items,
        usage,
        stop_reason,
    } = parse_response(&fixture("ordinary.json")).unwrap()
    else {
        panic!()
    };
    assert_eq!(items.len(), 2);
    assert_eq!(stop_reason, ModelStopReason::ToolUse);
    assert_eq!(usage.input_tokens, TokenCount::Known(13));
    assert_eq!(usage.cache_read_tokens, Some(TokenCount::Known(3)));

    let InvokeOutcome::Completed {
        items,
        usage,
        stop_reason,
    } = parse_sse(&fixture("complete.sse")).unwrap()
    else {
        panic!()
    };
    assert_eq!(items.len(), 2);
    assert_eq!(stop_reason, ModelStopReason::ToolUse);
    assert_eq!(usage.input_tokens, TokenCount::Known(6));
    assert_eq!(usage.output_tokens, TokenCount::Known(5));
}

#[test]
fn eof_is_transport_interruption_and_unclosed_block_fails_terminal() {
    let InvokeOutcome::Interrupted { partial_items, .. } =
        parse_sse(&fixture("truncated.sse")).unwrap()
    else {
        panic!()
    };
    assert_eq!(partial_items.len(), 1);
    let malformed = String::from_utf8(fixture("complete.sse")).unwrap().replace(
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "",
    );
    assert_eq!(
        parse_sse(malformed.as_bytes()),
        Err(AnthropicAdapterError::Invariant)
    );
}
