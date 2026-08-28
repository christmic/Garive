use std::{fs, path::PathBuf};

use garive_llm::{
    InvokeOutcome, ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings,
    ModelRequest, ModelRequestId, ModelRole, ModelStopReason, ModelTargetId, TextMode, TokenCount,
    ToolDescriptor,
};
use garive_llm_openai::{parse_response, parse_sse, render_request, OpenAiAdapterError};
use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/providers/openai/responses");
    fs::read(root.join(name)).unwrap()
}

fn request() -> ModelRequest {
    ModelRequest {
        request_id: ModelRequestId::new("request-1"),
        target_id: ModelTargetId::new("gpt-5.4"),
        required_capabilities: vec![ModelCapability::Text, ModelCapability::Tools, ModelCapability::Streaming],
        input_items: vec![ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text("hello".into())],
        }],
        tools: vec![ToolDescriptor {
            name: "weather".into(), description: "Lookup weather".into(),
            definition_revision: "1".into(),
            input_schema_json: r#"{"type":"object","properties":{"city":{"type":"string"}},"required":["city"],"additionalProperties":false}"#.into(),
            strict: true,
        }],
        output: ModelOutputSettings { max_output_tokens: Some(128), text_mode: TextMode::Plain,
            reasoning_visibility: false },
        trace_metadata: vec![("trace".into(), "fixture".into())],
    }
}

#[test]
fn request_matches_official_shape_fixture() {
    let expected: Value = serde_json::from_slice(&fixture("request.json")).unwrap();
    assert_eq!(render_request(&request(), true).unwrap(), expected);
}

#[test]
fn ordinary_and_complete_stream_normalize_identically() {
    let ordinary = parse_response(&fixture("ordinary.json")).unwrap();
    let InvokeOutcome::Completed {
        items,
        usage,
        stop_reason,
    } = &ordinary
    else {
        panic!()
    };
    assert_eq!(items.len(), 2);
    assert_eq!(*stop_reason, ModelStopReason::ToolUse);
    assert_eq!(usage.input_tokens, TokenCount::Known(10));
    assert_eq!(usage.cache_read_tokens, Some(TokenCount::Known(3)));

    let stream = parse_sse(&fixture("complete.sse")).unwrap();
    let InvokeOutcome::Completed {
        items,
        usage,
        stop_reason,
    } = stream
    else {
        panic!()
    };
    assert_eq!(items.len(), 1);
    assert_eq!(stop_reason, ModelStopReason::EndTurn);
    assert_eq!(usage.input_tokens, TokenCount::Known(4));
}

#[test]
fn eof_is_transport_interruption_and_bad_sequence_fails_closed() {
    let outcome = parse_sse(&fixture("truncated.sse")).unwrap();
    let InvokeOutcome::Interrupted { partial_items, .. } = outcome else {
        panic!()
    };
    assert_eq!(partial_items.len(), 1);
    let malformed = String::from_utf8(fixture("complete.sse"))
        .unwrap()
        .replacen("\"sequence_number\":3", "\"sequence_number\":2", 1);
    assert_eq!(
        parse_sse(malformed.as_bytes()),
        Err(OpenAiAdapterError::Invariant)
    );
}
