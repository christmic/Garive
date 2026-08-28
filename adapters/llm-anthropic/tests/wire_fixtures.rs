use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCapability, ModelInputContent, ModelInputItem,
    ModelOutputSettings, ModelRequest, ModelRequestId, ModelRole, ModelStopReason, ModelTargetId,
    TextMode, TokenCount, ToolDescriptor,
};
use garive_llm_anthropic::{
    classify_http_error, parse_response, parse_sse, render_request, AnthropicAdapterError,
    HttpErrorAction,
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime},
};

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

#[test]
fn output_limit_is_a_partial_factual_terminal() {
    let body = br#"{"content":[{"type":"text","text":"partial"}],"stop_reason":"max_tokens","usage":{"input_tokens":2,"output_tokens":4}}"#;
    let InvokeOutcome::Interrupted {
        kind,
        partial_items,
        usage,
    } = parse_response(body).unwrap()
    else {
        panic!()
    };
    assert_eq!(kind, InterruptionKind::OutputLimit);
    assert_eq!(partial_items.len(), 1);
    assert_eq!(usage.output_tokens, TokenCount::Known(4));
}

#[test]
fn stream_error_is_a_verified_unavailable_terminal() {
    let outcome = parse_sse(&fixture("stream-error.sse")).unwrap();
    assert_eq!(
        outcome,
        InvokeOutcome::Unavailable {
            kind: garive_llm::UnavailableKind::ModelUnavailable,
            retry_after: None,
        }
    );
}

#[test]
fn ordinary_and_stream_preserve_thinking_evidence() {
    let ordinary = parse_response(&fixture("thinking.json")).unwrap();
    let streamed = parse_sse(&fixture("thinking.sse")).unwrap();
    assert_eq!(ordinary, streamed);
    let InvokeOutcome::Completed { items, .. } = ordinary else {
        panic!()
    };
    assert_eq!(items.len(), 4);
    assert!(matches!(
        &items[1],
        garive_llm::ModelItem::Reasoning {
            content: garive_llm::ReasoningContent::OpaqueReference(value)
        } if value == "opaque-signature"
    ));
}

#[test]
fn shared_http_error_cases_have_exact_terminals() {
    let fixture: Value = serde_json::from_slice(&fixture("errors.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let action = classify_http_error(
            case["status"].as_u64().unwrap() as u16,
            case["retry_after"].as_str(),
            &serde_json::to_vec(&case["body"]).unwrap(),
            true,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(render_action(action), case["expected"].as_str().unwrap());
    }
}

#[test]
fn retry_after_supports_delta_seconds_and_http_date() {
    let body = br#"{"type":"error","error":{"type":"rate_limit_error","message":"busy"}}"#;
    assert_eq!(
        classify_http_error(429, Some("2"), body, false, SystemTime::UNIX_EPOCH).unwrap(),
        HttpErrorAction::Retry {
            retry_after: Some(Duration::from_secs(2))
        }
    );
    assert_eq!(
        classify_http_error(
            429,
            Some("Thu, 01 Jan 1970 00:00:03 GMT"),
            body,
            false,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap(),
        HttpErrorAction::Retry {
            retry_after: Some(Duration::from_secs(3))
        }
    );
}

fn render_action(action: HttpErrorAction) -> String {
    match action {
        HttpErrorAction::Retry { retry_after } => {
            format!("retry:{}", retry_after.unwrap().as_secs())
        }
        HttpErrorAction::Terminal(InvokeOutcome::Rejected { kind, .. }) => format!(
            "rejected:{}",
            match kind {
                garive_llm::RejectionKind::ContextOverflow => "context-overflow",
                garive_llm::RejectionKind::Authentication => "authentication",
                garive_llm::RejectionKind::ContentPolicy => "content-policy",
            }
        ),
        HttpErrorAction::Terminal(InvokeOutcome::Unavailable { kind, retry_after }) => match kind {
            garive_llm::UnavailableKind::RateLimited => format!(
                "unavailable:rate-limited:{}",
                retry_after.unwrap().as_secs()
            ),
            garive_llm::UnavailableKind::ModelUnavailable => "unavailable:model-unavailable".into(),
            garive_llm::UnavailableKind::CircuitOpen => "unavailable:circuit-open".into(),
        },
        HttpErrorAction::Terminal(_) => "unexpected".into(),
    }
}
