use std::{fs, path::PathBuf};

use garive_llm::{
    ModelCapability, ModelInputContent, ModelInputItem, ModelItem, ModelOutputKind,
    ModelOutputSettings, ModelRequest, ModelRequestId, ModelRole, ModelStreamEvent, ModelTargetId,
    ModelUsage, StreamInvariantError, StreamValidator, TextMode, TokenCount, ToolDescriptor,
    UsageSource,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/model-request-stream.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn request() -> ModelRequest {
    ModelRequest {
        request_id: ModelRequestId::new("request-1"),
        target_id: ModelTargetId::new("primary"),
        required_capabilities: vec![ModelCapability::Text, ModelCapability::Streaming],
        input_items: vec![ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text("hello".into())],
        }],
        tools: vec![ToolDescriptor {
            name: "lookup".into(),
            description: "look up a value".into(),
            definition_revision: "1".into(),
            input_schema_json: "{}".into(),
            strict: true,
        }],
        output: ModelOutputSettings {
            max_output_tokens: Some(100),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        trace_metadata: vec![("trace".into(), "one".into())],
    }
}

fn kind(value: &str) -> ModelOutputKind {
    match value {
        "text" => ModelOutputKind::Text,
        "refusal" => ModelOutputKind::Refusal,
        "reasoning" => ModelOutputKind::Reasoning,
        other => panic!("unknown fixture kind: {other}"),
    }
}

fn event(encoded: &str) -> ModelStreamEvent {
    let parts: Vec<_> = encoded.split(':').collect();
    let index = || parts[1].parse().unwrap();
    match parts[0] {
        "start" => ModelStreamEvent::OutputItemStarted {
            output_index: index(),
            kind: kind(parts[2]),
        },
        "text" => ModelStreamEvent::TextDelta {
            output_index: index(),
            delta: parts[2].into(),
        },
        "refusal" => ModelStreamEvent::RefusalDelta {
            output_index: index(),
            delta: parts[2].into(),
        },
        "reasoning" => ModelStreamEvent::ReasoningDelta {
            output_index: index(),
            delta: parts[2].into(),
        },
        "complete" => ModelStreamEvent::OutputItemCompleted {
            output_index: index(),
            item: match parts[2] {
                "text" => ModelItem::Text { text: "a".into() },
                "refusal" => ModelItem::Refusal { text: "no".into() },
                "reasoning" => ModelItem::Reasoning {
                    content: garive_llm::ReasoningContent::ModelVisible("r".into()),
                },
                other => panic!("unknown completed kind: {other}"),
            },
        },
        "usage" => ModelStreamEvent::UsageUpdated {
            usage: ModelUsage {
                input_tokens: TokenCount::Known(1),
                output_tokens: TokenCount::Known(1),
                cache_read_tokens: None,
                cache_write_tokens: None,
                source: UsageSource::ProviderReported,
            },
        },
        other => panic!("unknown fixture event: {other}"),
    }
}

#[test]
fn rust_consumes_every_request_case() {
    let document = fixture();
    let cases = document["request_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let mut value = request();
        match case["mutation"].as_str().unwrap() {
            "none" => {}
            "empty-request-id" => value.request_id = ModelRequestId::new(""),
            "duplicate-capability" => value.required_capabilities.push(ModelCapability::Text),
            "duplicate-tool" => value.tools.push(value.tools[0].clone()),
            "zero-output-limit" => value.output.max_output_tokens = Some(0),
            other => panic!("unknown mutation: {other}"),
        }
        let actual = value.validate().map_or_else(|error| error.code(), |_| "ok");
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}

#[test]
fn rust_consumes_every_stream_case() {
    let document = fixture();
    let cases = document["stream_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let mut validator = StreamValidator::default();
        let mut result = Ok(());
        for encoded in case["events"].as_array().unwrap() {
            result = validator.accept(&event(encoded.as_str().unwrap()));
            if result.is_err() {
                break;
            }
        }
        let actual = result.map_or_else(|error: StreamInvariantError| error.code(), |_| "ok");
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}
