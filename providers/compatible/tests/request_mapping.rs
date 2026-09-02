use std::collections::{BTreeMap, BTreeSet};

use garive_llm::{
    ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings, ModelRequest,
    ModelRequestId, ModelRole, ModelTargetId, TextMode, ToolDescriptor,
};
use garive_provider_compatible::{
    map_messages_request, map_responses_request, normalize_responses, wire_tool_name,
    CompatibleProviderError, MessagesDeployment, ProtocolErrorPolicy, ResponsesDeployment,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/providers/compatible-mapping-v1.json"
    ))
    .expect("valid shared fixture")
}

fn capabilities(values: &[Value]) -> Vec<ModelCapability> {
    values
        .iter()
        .map(|value| match value.as_str().expect("capability string") {
            "text" => ModelCapability::Text,
            "tools" => ModelCapability::Tools,
            "json_output" => ModelCapability::JsonOutput,
            "streaming" => ModelCapability::Streaming,
            other => panic!("unsupported fixture capability {other}"),
        })
        .collect()
}

fn neutral_request(value: &Value) -> ModelRequest {
    let input_items = value["input"]
        .as_array()
        .expect("input array")
        .iter()
        .map(|item| match item["kind"].as_str().expect("item kind") {
            "message" => ModelInputItem::Message {
                role: match item["role"].as_str().expect("role") {
                    "system" => ModelRole::System,
                    "developer" => ModelRole::Developer,
                    "user" => ModelRole::User,
                    "assistant" => ModelRole::Assistant,
                    other => panic!("unsupported fixture role {other}"),
                },
                content: vec![ModelInputContent::Text(
                    item["text"].as_str().expect("message text").to_owned(),
                )],
            },
            "tool_observation" => ModelInputItem::ToolObservation {
                model_call_id: item["model_call_id"]
                    .as_str()
                    .expect("model call id")
                    .to_owned(),
                result_json: item["result_json"].to_string(),
            },
            other => panic!("unsupported fixture item {other}"),
        })
        .collect();
    let tools = value["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| ToolDescriptor {
            name: tool["name"].as_str().expect("tool name").to_owned(),
            description: tool["description"]
                .as_str()
                .expect("tool description")
                .to_owned(),
            definition_revision: tool["revision"].as_str().expect("tool revision").to_owned(),
            input_schema_json: tool["schema"].to_string(),
            strict: tool["strict"].as_bool().expect("tool strict"),
        })
        .collect();
    let text_mode = match value["text_mode"]["kind"].as_str().expect("text mode") {
        "plain" => TextMode::Plain,
        "json_object" => TextMode::JsonObject,
        "json_schema" => TextMode::JsonSchema {
            schema_json: value["text_mode"]["schema"].to_string(),
        },
        other => panic!("unsupported fixture text mode {other}"),
    };
    let metadata = value["metadata"]
        .as_object()
        .expect("metadata object")
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                value.as_str().expect("metadata string").to_owned(),
            )
        })
        .collect();
    ModelRequest {
        request_id: ModelRequestId::new(value["request_id"].as_str().expect("request id")),
        target_id: ModelTargetId::new(value["target_id"].as_str().expect("target id")),
        required_capabilities: capabilities(
            value["required_capabilities"]
                .as_array()
                .expect("capabilities array"),
        ),
        input_items,
        tools,
        output: ModelOutputSettings {
            max_output_tokens: value["max_output_tokens"].as_u64(),
            text_mode,
            reasoning_visibility: value["reasoning_visibility"]
                .as_bool()
                .expect("reasoning visibility"),
        },
        trace_metadata: metadata,
    }
}

fn response_deployment(value: &Value) -> ResponsesDeployment {
    ResponsesDeployment {
        target_id: value["target_id"].as_str().expect("target id").to_owned(),
        model_id: value["model_id"].as_str().expect("model id").to_owned(),
        capabilities: capabilities(value["capabilities"].as_array().expect("capabilities"))
            .into_iter()
            .collect::<BTreeSet<_>>(),
        default_max_output_tokens: value["default_max_output_tokens"].as_u64(),
        media_bindings: BTreeMap::new(),
        reasoning: None,
        error_policy: ProtocolErrorPolicy::default(),
    }
}

fn messages_deployment(value: &Value) -> MessagesDeployment {
    MessagesDeployment {
        target_id: value["target_id"].as_str().expect("target id").to_owned(),
        model_id: value["model_id"].as_str().expect("model id").to_owned(),
        capabilities: capabilities(value["capabilities"].as_array().expect("capabilities"))
            .into_iter()
            .collect::<BTreeSet<_>>(),
        default_max_output_tokens: value["default_max_output_tokens"].as_u64(),
        media_bindings: BTreeMap::new(),
        thinking: None,
        error_policy: ProtocolErrorPolicy::default(),
    }
}

#[test]
fn shared_responses_request_case_maps_without_protocol_extensions() {
    let fixture = fixture();
    let case = &fixture["request_cases"][0];
    let mapped = map_responses_request(
        &response_deployment(&fixture["deployments"]["responses"]),
        &neutral_request(&case["request"]),
    )
    .expect("fixture maps");
    let wire = serde_json::to_value(mapped).expect("serialize mapped request");
    let expected = &case["expected"];

    assert_eq!(wire["model"], expected["model"]);
    assert_eq!(wire["stream"], expected["stream"]);
    assert_eq!(wire["max_output_tokens"], expected["max_output_tokens"]);
    assert_eq!(wire["metadata"], expected["metadata"]);
    assert_eq!(wire["tools"][0]["name"], expected["tool_names"][0]);
    assert_eq!(wire["text"]["format"]["type"], "json_schema");
    assert!(wire.get("extensions").is_none());
}

#[test]
fn shared_messages_request_case_uses_leading_system_and_default_limit() {
    let fixture = fixture();
    let case = &fixture["request_cases"][1];
    let mapped = map_messages_request(
        &messages_deployment(&fixture["deployments"]["messages"]),
        &neutral_request(&case["request"]),
    )
    .expect("fixture maps");
    let wire = serde_json::to_value(mapped).expect("serialize mapped request");
    let expected = &case["expected"];

    assert_eq!(wire["model"], expected["model"]);
    assert_eq!(wire["stream"], expected["stream"]);
    assert_eq!(wire["max_tokens"], expected["max_output_tokens"]);
    assert_eq!(wire["system"][0]["text"], expected["system_blocks"][0]);
    assert_eq!(wire["system"][1]["text"], expected["system_blocks"][1]);
    assert_eq!(wire["messages"][1]["content"][0]["type"], "tool_result");
    assert_eq!(wire["output_config"]["format"]["type"], "json_schema");
}

#[test]
fn tool_call_history_is_correlated_and_uses_wire_safe_names() {
    let fixture = fixture();
    for value in fixture["tool_name_mapping_cases"].as_array().unwrap() {
        assert_eq!(
            wire_tool_name(value["neutral"].as_str().unwrap()),
            value["wire"]
        );
    }
    for protocol in ["responses", "messages"] {
        let case_index = usize::from(protocol == "messages");
        let mut request = neutral_request(&fixture["request_cases"][case_index]["request"]);
        request.tools[0].name = "garive.workspace.read_text".into();
        let observation = request.input_items.pop().expect("observation");
        request.input_items.push(ModelInputItem::ToolIntent {
            model_call_id: "call-0".into(),
            tool_name: "garive.workspace.read_text".into(),
            arguments_json: "{\"path\":\"README.md\"}".into(),
        });
        request.input_items.push(observation);

        let wire = if protocol == "responses" {
            serde_json::to_value(
                map_responses_request(
                    &response_deployment(&fixture["deployments"]["responses"]),
                    &request,
                )
                .unwrap(),
            )
            .unwrap()
        } else {
            serde_json::to_value(
                map_messages_request(
                    &messages_deployment(&fixture["deployments"]["messages"]),
                    &request,
                )
                .unwrap(),
            )
            .unwrap()
        };
        let encoded = wire.to_string();
        assert!(!encoded.contains("garive.workspace.read_text"));
        assert!(encoded.contains("garive_"));
        assert!(encoded.contains("call-0"));
        if protocol == "messages" {
            assert_eq!(wire["messages"][1]["role"], "assistant");
            assert_eq!(wire["messages"][1]["content"][0]["type"], "tool_use");
            assert_eq!(wire["messages"][2]["content"][0]["type"], "tool_result");
        } else {
            assert_eq!(wire["input"][2]["type"], "function_call");
            assert_eq!(wire["input"][3]["type"], "function_call_output");
        }
    }
}

#[test]
fn messages_rejects_late_instruction_and_metadata() {
    let fixture = fixture();
    let deployment = messages_deployment(&fixture["deployments"]["messages"]);
    let mut request = neutral_request(&fixture["request_cases"][1]["request"]);
    request.input_items.push(ModelInputItem::Message {
        role: ModelRole::Developer,
        content: vec![ModelInputContent::Text("late".to_owned())],
    });
    assert_eq!(
        map_messages_request(&deployment, &request),
        Err(CompatibleProviderError::UnsupportedInput)
    );

    request.input_items.pop();
    request.trace_metadata.push(("trace".into(), "x".into()));
    assert_eq!(
        map_messages_request(&deployment, &request),
        Err(CompatibleProviderError::UnsupportedMetadata)
    );
}

#[test]
fn messages_admits_runtime_trace_identity_without_sending_vendor_metadata() {
    let fixture = fixture();
    let deployment = messages_deployment(&fixture["deployments"]["messages"]);
    let mut request = neutral_request(&fixture["request_cases"][1]["request"]);
    request.trace_metadata = vec![
        ("turn_id".into(), "turn-1".into()),
        ("execution_id".into(), "execution-1".into()),
    ];

    let mapped = map_messages_request(&deployment, &request).expect("runtime trace is internal");

    assert_eq!(mapped.metadata, None);
}

#[test]
fn every_shared_failure_case_returns_its_stable_code() {
    let fixture = fixture();
    let responses = response_deployment(&fixture["deployments"]["responses"]);
    let messages = messages_deployment(&fixture["deployments"]["messages"]);
    for case in fixture["failure_cases"].as_array().expect("failure cases") {
        let name = case["name"].as_str().expect("failure name");
        let failure = match name {
            "target-mismatch" => {
                let mut request = neutral_request(&fixture["request_cases"][0]["request"]);
                request.target_id = ModelTargetId::new("wrong");
                map_responses_request(&responses, &request).expect_err("target mismatch")
            }
            "unsupported-capability" => {
                let mut request = neutral_request(&fixture["request_cases"][0]["request"]);
                request.required_capabilities.push(ModelCapability::Vision);
                map_responses_request(&responses, &request).expect_err("capability")
            }
            "invalid-tool-schema" => {
                let mut request = neutral_request(&fixture["request_cases"][0]["request"]);
                request.tools[0].input_schema_json = "[]".into();
                map_responses_request(&responses, &request).expect_err("schema")
            }
            "messages-late-instruction" => {
                let mut request = neutral_request(&fixture["request_cases"][1]["request"]);
                request.input_items.push(ModelInputItem::Message {
                    role: ModelRole::Developer,
                    content: vec![ModelInputContent::Text("late".into())],
                });
                map_messages_request(&messages, &request).expect_err("late instruction")
            }
            "messages-metadata" => {
                let mut request = neutral_request(&fixture["request_cases"][1]["request"]);
                request.trace_metadata.push(("trace".into(), "x".into()));
                map_messages_request(&messages, &request).expect_err("metadata")
            }
            "reasoning-without-profile" => {
                let mut request = neutral_request(&fixture["request_cases"][0]["request"]);
                request.output.reasoning_visibility = true;
                map_responses_request(&responses, &request).expect_err("reasoning profile")
            }
            "unadmitted-extension" => {
                let response: garive_openai_responses::Response = serde_json::from_value(json!({
                    "id":"response","created_at":1.0,"error":null,"incomplete_details":null,
                    "instructions":null,"metadata":null,"model":"model","object":"response",
                    "output":[{"type":"hosted_tool_call","id":"hosted"}],
                    "parallel_tool_calls":false,"temperature":null,"tool_choice":"auto",
                    "tools":[],"top_p":null,"status":"completed","usage":null
                }))
                .expect("extension response");
                normalize_responses(&response, false).expect_err("extension")
            }
            "messages-missing-output-limit" => {
                let mut deployment = messages.clone();
                deployment.default_max_output_tokens = None;
                let request = neutral_request(&fixture["request_cases"][1]["request"]);
                map_messages_request(&deployment, &request).expect_err("output limit")
            }
            other => panic!("unhandled shared failure {other}"),
        };
        assert_eq!(failure.code(), case["code"].as_str().expect("failure code"));
    }
}
