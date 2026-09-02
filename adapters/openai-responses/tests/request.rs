use garive_adapter_openai_responses::{
    CreateResponseRequest, FunctionCall, FunctionCallOutput, FunctionOutput, FunctionTool, Header,
    ImageDetail, InputContent, InputItem, ItemStatus, MessageRole, ReasoningConfig,
    ReasoningEffort, ReasoningSummary, ResponseInput, ResponseTextConfig, ResponseTool,
    ResponsesAdapter, ResponsesAdapterConfig, ResponsesAdapterError, TextFormat, ToolChoice,
    ToolChoiceMode,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn adapter() -> ResponsesAdapter {
    ResponsesAdapter::new(
        ResponsesAdapterConfig::new(
            "https://compatible.example/inference/responses",
            vec![Header::new("authorization", "fixture-secret", true).unwrap()],
        )
        .unwrap(),
    )
}

fn official_request() -> CreateResponseRequest {
    let mut request = CreateResponseRequest::new(
        "gpt-5.4",
        ResponseInput::Items(vec![InputItem::Message {
            role: MessageRole::User,
            content: vec![InputContent::InputText {
                text: "hello".into(),
            }],
        }]),
        true,
    );
    request.max_output_tokens = Some(128);
    request.metadata = BTreeMap::from([("trace".into(), "fixture".into())]);
    request.tools = vec![ResponseTool::Function(FunctionTool {
        name: "weather".into(),
        description: Some("Lookup weather".into()),
        parameters: serde_json::from_value(json!({
            "type":"object",
            "properties":{"city":{"type":"string"}},
            "required":["city"],
            "additionalProperties":false
        }))
        .unwrap(),
        strict: true,
    })];
    request
        .extensions
        .insert("store".into(), Value::Bool(false));
    request
}

#[test]
fn prepare_matches_pinned_official_create_shape() {
    let expected: Value = serde_json::from_slice(
        &fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/protocols/openai-responses/request.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let request = adapter().prepare(&official_request()).unwrap();
    assert_eq!(request.method(), "POST");
    assert_eq!(
        request.uri(),
        "https://compatible.example/inference/responses"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(request.body()).unwrap(),
        expected
    );
    assert!(request
        .headers()
        .iter()
        .any(|header| header.name() == "accept" && header.value() == "text/event-stream"));
    assert!(!format!("{request:?}").contains("fixture-secret"));
}

#[test]
fn request_validation_rejects_ambiguous_images_and_field_collisions() {
    let mut request = CreateResponseRequest::new(
        "model",
        ResponseInput::Items(vec![InputItem::Message {
            role: MessageRole::User,
            content: vec![InputContent::InputImage {
                image_url: Some("https://example.test/image.png".into()),
                file_id: Some("file-1".into()),
                detail: Some(ImageDetail::High),
            }],
        }]),
        false,
    );
    assert_eq!(
        request.validate(),
        Err(ResponsesAdapterError::InvalidRequest(
            "Responses image requires exactly one reference"
        ))
    );
    if let ResponseInput::Items(items) = &mut request.input {
        let InputItem::Message { content, .. } = &mut items[0] else {
            panic!()
        };
        let InputContent::InputImage { file_id, .. } = &mut content[0] else {
            panic!()
        };
        *file_id = None;
    }
    request.extensions.insert("model".into(), json!("other"));
    assert_eq!(
        request.validate(),
        Err(ResponsesAdapterError::InvalidRequest(
            "Responses extension collides with a typed field"
        ))
    );
}

#[test]
fn request_validation_rejects_non_finite_or_out_of_range_sampling() {
    let mut request =
        CreateResponseRequest::new("model", ResponseInput::Text("hello".into()), false);
    request.temperature = Some(f64::NAN);
    assert_eq!(
        request.validate(),
        Err(ResponsesAdapterError::InvalidRequest(
            "invalid Responses temperature"
        ))
    );
    request.temperature = Some(1.0);
    request.top_p = Some(1.5);
    assert_eq!(
        request.validate(),
        Err(ResponsesAdapterError::InvalidRequest(
            "invalid Responses top_p"
        ))
    );
}

#[test]
fn portable_request_unions_encode_as_official_shapes() {
    let mut request = CreateResponseRequest::new(
        "model",
        ResponseInput::Items(vec![InputItem::FunctionCallOutput(FunctionCallOutput {
            call_id: "call_1".into(),
            output: FunctionOutput::Content(vec![InputContent::InputImage {
                image_url: None,
                file_id: Some("file_1".into()),
                detail: Some(ImageDetail::Low),
            }]),
            status: Some(ItemStatus::Completed),
        })]),
        false,
    );
    request.tool_choice = Some(ToolChoice::Mode(ToolChoiceMode::Required));
    request.text = Some(ResponseTextConfig {
        format: TextFormat::JsonSchema {
            name: "answer".into(),
            description: None,
            schema: serde_json::from_value(json!({"type":"object"})).unwrap(),
            strict: true,
        },
    });
    request.reasoning = Some(ReasoningConfig {
        effort: Some(ReasoningEffort::Xhigh),
        summary: Some(ReasoningSummary::Detailed),
    });
    let value: Value = serde_json::from_slice(adapter().prepare(&request).unwrap().body()).unwrap();
    assert_eq!(value["input"][0]["output"][0]["file_id"], "file_1");
    assert_eq!(value["tool_choice"], "required");
    assert_eq!(value["text"]["format"]["type"], "json_schema");
    assert_eq!(value["reasoning"]["effort"], "xhigh");

    request.input = ResponseInput::Items(vec![InputItem::FunctionCallOutput(FunctionCallOutput {
        call_id: "call_1".into(),
        output: FunctionOutput::Content(vec![]),
        status: None,
    })]);
    assert!(request.validate().is_err());
}

#[test]
fn prior_function_call_encodes_for_tool_result_correlation() {
    let request = CreateResponseRequest::new(
        "model",
        ResponseInput::Items(vec![
            InputItem::FunctionCall(FunctionCall {
                call_id: "call_1".into(),
                name: "read_text".into(),
                arguments: "{\"path\":\"README.md\"}".into(),
            }),
            InputItem::FunctionCallOutput(FunctionCallOutput {
                call_id: "call_1".into(),
                output: FunctionOutput::Text("ok".into()),
                status: None,
            }),
        ]),
        false,
    );
    let value: Value = serde_json::from_slice(adapter().prepare(&request).unwrap().body()).unwrap();
    assert_eq!(value["input"][0]["type"], "function_call");
    assert_eq!(value["input"][0]["call_id"], "call_1");
    assert_eq!(value["input"][1]["type"], "function_call_output");
}
