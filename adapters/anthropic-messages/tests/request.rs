use garive_adapter_anthropic_messages::{
    CreateMessageRequest, Header, Message, MessageContent, MessageRole, MessagesAdapter,
    MessagesAdapterConfig, Metadata, SystemPrompt, TextBlock, TextBlockType, Tool,
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
