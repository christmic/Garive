//! Strict OpenAI Responses wire codec. HTTP ownership remains in Runtime.

#![forbid(unsafe_code)]

use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelInputContent, ModelInputItem, ModelItem, ModelRequest,
    ModelRole, ModelStopReason, ModelUsage, ReasoningContent, TextMode, TokenCount, UsageSource,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiAdapterError {
    InvalidRequest,
    UnsupportedCapability,
    InvalidJson,
    Invariant,
}

pub fn render_request(request: &ModelRequest, stream: bool) -> Result<Value, OpenAiAdapterError> {
    request
        .validate()
        .map_err(|_| OpenAiAdapterError::InvalidRequest)?;
    if request.trace_metadata.len() > 16 {
        return Err(OpenAiAdapterError::InvalidRequest);
    }
    let mut input = Vec::new();
    for item in &request.input_items {
        let ModelInputItem::Message { role, content } = item else {
            return Err(OpenAiAdapterError::UnsupportedCapability);
        };
        let content = content
            .iter()
            .map(|content| match content {
                ModelInputContent::Text(text) => Ok(json!({"type":"input_text","text":text})),
                ModelInputContent::MediaReference {
                    media_kind: garive_llm::MediaKind::Image,
                    reference,
                    ..
                } => Ok(json!({"type":"input_image","image_url":reference})),
                _ => Err(OpenAiAdapterError::UnsupportedCapability),
            })
            .collect::<Result<Vec<_>, _>>()?;
        input.push(json!({"type":"message","role":role_name(*role),"content":content}));
    }
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            let parameters: Value = serde_json::from_str(&tool.input_schema_json)
                .map_err(|_| OpenAiAdapterError::InvalidRequest)?;
            Ok(
                json!({"type":"function","name":tool.name,"description":tool.description,
            "parameters":parameters,"strict":tool.strict}),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut body = Map::new();
    body.insert("model".into(), json!(request.target_id.as_str()));
    body.insert("input".into(), Value::Array(input));
    body.insert("stream".into(), json!(stream));
    body.insert("store".into(), json!(false));
    if let Some(limit) = request.output.max_output_tokens {
        body.insert("max_output_tokens".into(), json!(limit));
    }
    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools));
    }
    if !request.trace_metadata.is_empty() {
        body.insert(
            "metadata".into(),
            Value::Object(
                request
                    .trace_metadata
                    .iter()
                    .map(|(key, value)| (key.clone(), json!(value)))
                    .collect(),
            ),
        );
    }
    match &request.output.text_mode {
        TextMode::Plain => {}
        TextMode::JsonObject => {
            body.insert("text".into(), json!({"format":{"type":"json_object"}}));
        }
        TextMode::JsonSchema { schema_json } => {
            let schema: Value = serde_json::from_str(schema_json)
                .map_err(|_| OpenAiAdapterError::InvalidRequest)?;
            body.insert(
                "text".into(),
                json!({"format":{"type":"json_schema","name":"garive_output",
                "schema":schema,"strict":true}}),
            );
        }
    }
    Ok(Value::Object(body))
}

pub fn parse_response(bytes: &[u8]) -> Result<InvokeOutcome, OpenAiAdapterError> {
    let response: Value =
        serde_json::from_slice(bytes).map_err(|_| OpenAiAdapterError::InvalidJson)?;
    if response["status"] != "completed" {
        return Err(OpenAiAdapterError::Invariant);
    }
    let items = parse_items(&response["output"])?;
    let usage = parse_usage(&response["usage"])?;
    let stop_reason = if items
        .iter()
        .any(|item| matches!(item, ModelItem::ToolIntent { .. }))
    {
        ModelStopReason::ToolUse
    } else if items
        .iter()
        .any(|item| matches!(item, ModelItem::Refusal { .. }))
    {
        ModelStopReason::Refusal
    } else {
        ModelStopReason::EndTurn
    };
    Ok(InvokeOutcome::Completed {
        items,
        usage,
        stop_reason,
    })
}

pub fn parse_sse(bytes: &[u8]) -> Result<InvokeOutcome, OpenAiAdapterError> {
    let text = std::str::from_utf8(bytes).map_err(|_| OpenAiAdapterError::InvalidJson)?;
    let mut previous = None;
    let mut assembled = BTreeMap::<u64, String>::new();
    let mut terminal = None;
    for line in text.lines().filter_map(|line| line.strip_prefix("data: ")) {
        if terminal.is_some() {
            return Err(OpenAiAdapterError::Invariant);
        }
        let event: Value =
            serde_json::from_str(line).map_err(|_| OpenAiAdapterError::InvalidJson)?;
        let sequence = event["sequence_number"]
            .as_u64()
            .ok_or(OpenAiAdapterError::Invariant)?;
        if previous.is_some_and(|value| sequence <= value) {
            return Err(OpenAiAdapterError::Invariant);
        }
        previous = Some(sequence);
        match event["type"]
            .as_str()
            .ok_or(OpenAiAdapterError::Invariant)?
        {
            "response.output_text.delta" => {
                let index = event["output_index"]
                    .as_u64()
                    .ok_or(OpenAiAdapterError::Invariant)?;
                let delta = event["delta"]
                    .as_str()
                    .ok_or(OpenAiAdapterError::Invariant)?;
                assembled.entry(index).or_default().push_str(delta);
            }
            "response.output_text.done" => {
                let index = event["output_index"]
                    .as_u64()
                    .ok_or(OpenAiAdapterError::Invariant)?;
                if assembled.get(&index).map(String::as_str) != event["text"].as_str() {
                    return Err(OpenAiAdapterError::Invariant);
                }
            }
            "response.completed" => {
                terminal = Some(parse_response(event["response"].to_string().as_bytes())?)
            }
            "response.failed" => return Err(OpenAiAdapterError::Invariant),
            _ => {}
        }
    }
    if let Some(outcome) = terminal {
        return Ok(outcome);
    }
    Ok(InvokeOutcome::Interrupted {
        kind: InterruptionKind::Transport,
        partial_items: assembled
            .into_values()
            .map(|text| ModelItem::Text { text })
            .collect(),
        usage: unknown_usage(),
    })
}

fn parse_items(value: &Value) -> Result<Vec<ModelItem>, OpenAiAdapterError> {
    let values = value.as_array().ok_or(OpenAiAdapterError::Invariant)?;
    let mut items = Vec::new();
    for value in values {
        match value["type"]
            .as_str()
            .ok_or(OpenAiAdapterError::Invariant)?
        {
            "message" => {
                for content in value["content"]
                    .as_array()
                    .ok_or(OpenAiAdapterError::Invariant)?
                {
                    match content["type"]
                        .as_str()
                        .ok_or(OpenAiAdapterError::Invariant)?
                    {
                        "output_text" => items.push(ModelItem::Text {
                            text: required_text(content, "text")?,
                        }),
                        "refusal" => items.push(ModelItem::Refusal {
                            text: required_text(content, "refusal")?,
                        }),
                        _ => return Err(OpenAiAdapterError::UnsupportedCapability),
                    }
                }
            }
            "function_call" => items.push(ModelItem::ToolIntent {
                model_call_id: required_text(value, "call_id")?,
                tool_name: required_text(value, "name")?,
                arguments_json: required_text(value, "arguments")?,
            }),
            "reasoning" => {
                if let Some(reference) = value["encrypted_content"].as_str() {
                    items.push(ModelItem::Reasoning {
                        content: ReasoningContent::OpaqueReference(reference.into()),
                    });
                }
            }
            _ => return Err(OpenAiAdapterError::UnsupportedCapability),
        }
    }
    Ok(items)
}

fn parse_usage(value: &Value) -> Result<ModelUsage, OpenAiAdapterError> {
    let input = value["input_tokens"]
        .as_u64()
        .ok_or(OpenAiAdapterError::Invariant)?;
    let output = value["output_tokens"]
        .as_u64()
        .ok_or(OpenAiAdapterError::Invariant)?;
    if value["total_tokens"].as_u64() != input.checked_add(output) {
        return Err(OpenAiAdapterError::Invariant);
    }
    Ok(ModelUsage {
        input_tokens: TokenCount::Known(input),
        output_tokens: TokenCount::Known(output),
        cache_read_tokens: value["input_tokens_details"]["cached_tokens"]
            .as_u64()
            .map(TokenCount::Known),
        cache_write_tokens: value["input_tokens_details"]["cache_write_tokens"]
            .as_u64()
            .map(TokenCount::Known),
        source: UsageSource::ProviderReported,
    })
}

fn unknown_usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Unknown,
        output_tokens: TokenCount::Unknown,
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}
fn required_text(value: &Value, key: &str) -> Result<String, OpenAiAdapterError> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or(OpenAiAdapterError::Invariant)
}
const fn role_name(role: ModelRole) -> &'static str {
    match role {
        ModelRole::System => "system",
        ModelRole::Developer => "developer",
        ModelRole::User => "user",
        ModelRole::Assistant => "assistant",
    }
}
