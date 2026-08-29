use garive_adapter_anthropic_messages::{
    CacheControl, CacheControlType, CacheTtl, CitationsConfig, ContentBlock, CreateMessageRequest,
    DocumentSource, Effort, Header, ImageMediaType, ImageSource, JsonOutputFormat,
    JsonOutputFormatType, Message, MessageContent, MessageRole, MessagesAdapter,
    MessagesAdapterConfig, Metadata, OutputConfig, SystemPrompt, TextBlock, TextBlockType,
    TextMediaType, ThinkingConfig, ThinkingDisplay, Tool, ToolChoice,
};
use serde_json::{json, Map, Value};

fn adapter() -> MessagesAdapter {
    MessagesAdapter::new(
        MessagesAdapterConfig::new(
            "https://compatible.example/messages",
            vec![Header::new("x-api-key", "fixture-secret", true).unwrap()],
            "x-protocol-version",
            "fixture-version",
        )
        .unwrap(),
    )
}

#[test]
fn official_shape_fixture_is_encoded_without_vendor_defaults() {
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), json!({"city":{"type":"string"}}));
    schema.insert("required".into(), json!(["city"]));
    schema.insert("additionalProperties".into(), json!(false));
    let mut request = CreateMessageRequest::new(
        "claude-sonnet-4-5",
        128,
        vec![Message::new(
            MessageRole::User,
            MessageContent::Blocks(vec![
                garive_adapter_anthropic_messages::ContentBlock::Text {
                    text: "hello".into(),
                    cache_control: None,
                },
            ]),
        )],
        true,
    );
    request.system = Some(SystemPrompt::Blocks(vec![TextBlock {
        kind: TextBlockType::Text,
        text: "be concise".into(),
        cache_control: None,
        citations: None,
    }]));
    request.metadata = Some(Metadata {
        user_id: Some("fixture".into()),
    });
    request.tools.push(Tool {
        name: "weather".into(),
        input_schema: schema,
        description: Some("Lookup weather".into()),
        strict: None,
        cache_control: None,
    });

    let actual: Value =
        serde_json::from_slice(adapter().prepare(&request).unwrap().body()).unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/protocols/anthropic-messages/request.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn invalid_request_fails_before_transport_description() {
    let request = CreateMessageRequest::new("", 128, vec![], false);
    assert!(adapter().prepare(&request).is_err());
}

#[test]
fn request_descriptor_contains_only_constructed_configuration() {
    let request = CreateMessageRequest::new(
        "model-from-garive",
        0,
        vec![Message::new(
            MessageRole::User,
            MessageContent::Text("hello".into()),
        )],
        false,
    );
    let prepared = adapter().prepare(&request).unwrap();
    assert_eq!(prepared.method(), "POST");
    assert_eq!(prepared.uri(), "https://compatible.example/messages");
    assert!(prepared.headers().iter().any(|header| {
        header.name() == "x-protocol-version" && header.value() == "fixture-version"
    }));
}

#[test]
fn portable_source_output_thinking_and_choice_unions_are_typed() {
    let cache = CacheControl {
        kind: CacheControlType::Ephemeral,
        ttl: Some(CacheTtl::OneHour),
    };
    let blocks = vec![
        ContentBlock::Image {
            source: ImageSource::Base64 {
                media_type: ImageMediaType::Png,
                data: "aGVsbG8=".into(),
            },
            cache_control: Some(cache.clone()),
        },
        ContentBlock::Document {
            source: DocumentSource::Text {
                data: "document".into(),
                media_type: TextMediaType::Plain,
            },
            cache_control: Some(cache),
            citations: Some(CitationsConfig {
                enabled: Some(true),
            }),
            title: Some("title".into()),
            context: None,
        },
    ];
    let mut request = CreateMessageRequest::new(
        "model",
        2_048,
        vec![Message::new(
            MessageRole::User,
            MessageContent::Blocks(blocks),
        )],
        false,
    );
    request.tool_choice = Some(ToolChoice::None);
    request.thinking = Some(ThinkingConfig::Enabled {
        budget_tokens: 1_024,
        display: Some(ThinkingDisplay::Omitted),
    });
    request.output_config = Some(OutputConfig {
        effort: Some(Effort::Xhigh),
        format: Some(JsonOutputFormat {
            kind: JsonOutputFormatType::JsonSchema,
            schema: Map::from_iter([("type".into(), json!("object"))]),
        }),
    });
    let value: Value = serde_json::from_slice(adapter().prepare(&request).unwrap().body()).unwrap();
    assert_eq!(
        value["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(
        value["messages"][0]["content"][0]["cache_control"]["ttl"],
        "1h"
    );
    assert_eq!(value["tool_choice"]["type"], "none");
    assert_eq!(value["thinking"]["display"], "omitted");
    assert_eq!(value["output_config"]["format"]["type"], "json_schema");
}

#[test]
fn invalid_sources_and_thinking_budget_fail_before_transport() {
    let mut request = CreateMessageRequest::new(
        "model",
        1_024,
        vec![Message::new(
            MessageRole::User,
            MessageContent::Blocks(vec![ContentBlock::Image {
                source: ImageSource::Url { url: String::new() },
                cache_control: None,
            }]),
        )],
        false,
    );
    assert!(adapter().prepare(&request).is_err());
    request.messages = vec![Message::new(
        MessageRole::User,
        MessageContent::Text("hello".into()),
    )];
    request.thinking = Some(ThinkingConfig::Enabled {
        budget_tokens: 1_024,
        display: None,
    });
    assert!(adapter().prepare(&request).is_err());
}
