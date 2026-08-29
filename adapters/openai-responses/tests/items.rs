use garive_adapter_openai_responses::{OutputContent, ResponseOutputItem};
use serde_json::{json, Value};

#[test]
fn portable_output_items_decode_with_typed_content() {
    let items: Vec<ResponseOutputItem> = serde_json::from_value(json!([
        {"id":"msg_1","type":"message","status":"completed","role":"assistant",
         "content":[{"type":"output_text","text":"hello","annotations":[]}]},
        {"id":"call_1","type":"function_call","status":"completed",
         "call_id":"call_weather","name":"weather","arguments":"{\"city\":\"Paris\"}"},
        {"id":"reason_1","type":"reasoning","status":"completed",
         "summary":[{"type":"summary_text","text":"summary"}]}
    ]))
    .unwrap();
    assert!(matches!(items[0], ResponseOutputItem::Message(_)));
    let ResponseOutputItem::Message(message) = &items[0] else {
        panic!()
    };
    assert!(matches!(message.content[0], OutputContent::OutputText(_)));
    assert!(matches!(items[1], ResponseOutputItem::FunctionCall(_)));
    assert!(matches!(items[2], ResponseOutputItem::Reasoning(_)));
}

#[test]
fn hosted_or_future_items_and_content_are_lossless_extensions() {
    let value = json!({"id":"search_1","type":"web_search_call","status":"completed",
        "action":{"type":"search","query":"weather"}});
    let item: ResponseOutputItem = serde_json::from_value(value.clone()).unwrap();
    let ResponseOutputItem::Extension(extension) = &item else {
        panic!()
    };
    assert_eq!(extension.discriminator(), "web_search_call");
    assert_eq!(serde_json::to_value(item).unwrap(), value);

    let content_value = json!({"type":"future_content","payload":{"kept":true}});
    let content: OutputContent = serde_json::from_value(content_value.clone()).unwrap();
    assert!(matches!(content, OutputContent::Extension(_)));
    assert_eq!(serde_json::to_value(content).unwrap(), content_value);
}

#[test]
fn missing_discriminator_is_not_treated_as_an_extension() {
    assert!(serde_json::from_value::<ResponseOutputItem>(json!({"id":"missing"})).is_err());
    assert!(serde_json::from_value::<OutputContent>(Value::String("text".into())).is_err());
}
