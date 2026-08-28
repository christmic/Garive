use std::{fs, path::PathBuf};

use garive_llm::{
    InvokeOutcome, ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings,
    ModelRequest, ModelRequestId, ModelRole, ModelStopReason, ModelTargetId, TextMode, TokenCount,
    ToolDescriptor,
};
use garive_llm_openai::{classify_http_error, HttpErrorAction};
use garive_llm_openai::{
    parse_response, parse_sse, render_http_request, render_request, OpenAiAdapterError,
};
use serde_json::Value;
use std::time::{Duration, SystemTime};

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
    let http = render_http_request(&request(), true).unwrap();
    assert_eq!(http.method, "POST");
    assert_eq!(http.path, "/v1/responses");
    assert!(http.headers.contains(&("accept", "text/event-stream")));
    assert!(http
        .headers
        .iter()
        .all(|(name, _)| *name != "authorization"));
    assert_eq!(
        serde_json::from_slice::<Value>(&http.body).unwrap(),
        expected
    );
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

#[test]
fn incomplete_and_http_errors_follow_terminal_contract() {
    let outcome = parse_sse(&fixture("incomplete.sse")).unwrap();
    let InvokeOutcome::Interrupted {
        kind,
        partial_items,
        ..
    } = outcome
    else {
        panic!()
    };
    assert_eq!(kind, garive_llm::InterruptionKind::OutputLimit);
    assert_eq!(partial_items.len(), 1);

    let document: Value = serde_json::from_slice(&fixture("errors.json")).unwrap();
    for case in document["cases"].as_array().unwrap() {
        let action = classify_http_error(
            case["status"].as_u64().unwrap() as u16,
            case["retry_after"].as_str(),
            case["body"].to_string().as_bytes(),
            true,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(
            render_action(action),
            case["expected"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    let retry = classify_http_error(
        429,
        Some("2"),
        document["cases"][2]["body"].to_string().as_bytes(),
        false,
        SystemTime::UNIX_EPOCH,
    )
    .unwrap();
    assert_eq!(
        retry,
        HttpErrorAction::Retry {
            retry_after: Some(Duration::from_secs(2))
        }
    );
}

#[test]
fn composite_stream_preserves_reasoning_text_and_tool_arguments() {
    let InvokeOutcome::Completed {
        items, stop_reason, ..
    } = parse_sse(&fixture("composite.sse")).unwrap()
    else {
        panic!()
    };
    assert_eq!(items.len(), 4);
    assert_eq!(stop_reason, ModelStopReason::ToolUse);
    assert!(matches!(
        &items[0],
        garive_llm::ModelItem::Reasoning {
            content: garive_llm::ReasoningContent::ModelVisible(value)
        } if value == "plan"
    ));
}

#[test]
fn ordinary_incomplete_and_unknown_stream_event_are_exact() {
    let body = br#"{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[{"type":"message","content":[{"type":"output_text","text":"partial"}]}],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}}"#;
    let InvokeOutcome::Interrupted { kind, .. } = parse_response(body).unwrap() else {
        panic!()
    };
    assert_eq!(kind, garive_llm::InterruptionKind::OutputLimit);

    assert!(matches!(
        parse_response(&fixture("content-filter.json")).unwrap(),
        InvokeOutcome::Rejected {
            kind: garive_llm::RejectionKind::ContentPolicy,
            ..
        }
    ));
    assert!(matches!(
        parse_response(&fixture("refusal.json")).unwrap(),
        InvokeOutcome::Completed {
            stop_reason: ModelStopReason::Refusal,
            ..
        }
    ));

    let unknown = b"data: {\"type\":\"response.some_new_delta\",\"sequence_number\":0}\n\n";
    assert_eq!(
        parse_sse(unknown),
        Err(OpenAiAdapterError::UnsupportedCapability)
    );
    let malformed = String::from_utf8(fixture("composite.sse"))
        .unwrap()
        .replace(
            "\"part\":{\"type\":\"output_text\",\"text\":\"answer\",\"annotations\":[]}}",
            "\"part\":{\"type\":\"output_text\",\"text\":\"mismatch\",\"annotations\":[]}}",
        );
    assert_eq!(
        parse_sse(malformed.as_bytes()),
        Err(OpenAiAdapterError::Invariant)
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
