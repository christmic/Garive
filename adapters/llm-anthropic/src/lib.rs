//! Strict Anthropic Messages wire codec. HTTP headers and secrets remain in Runtime.

#![forbid(unsafe_code)]

use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelInputContent, ModelInputItem, ModelItem, ModelRequest,
    ModelRole, ModelStopReason, ModelUsage, ReasoningContent, TextMode, TokenCount, UsageSource,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnthropicAdapterError {
    InvalidRequest,
    UnsupportedCapability,
    InvalidJson,
    Invariant,
}

pub fn render_request(
    request: &ModelRequest,
    stream: bool,
) -> Result<Value, AnthropicAdapterError> {
    request
        .validate()
        .map_err(|_| AnthropicAdapterError::InvalidRequest)?;
    if !matches!(request.output.text_mode, TextMode::Plain) {
        return Err(AnthropicAdapterError::UnsupportedCapability);
    }
    let max_tokens = request
        .output
        .max_output_tokens
        .ok_or(AnthropicAdapterError::InvalidRequest)?;
    let mut system = Vec::new();
    let mut messages = Vec::new();
    let mut conversation_started = false;
    for item in &request.input_items {
        match item {
            ModelInputItem::Message {
                role: ModelRole::System | ModelRole::Developer,
                content,
            } => {
                if conversation_started {
                    return Err(AnthropicAdapterError::UnsupportedCapability);
                }
                for value in content {
                    system.push(text_block(value)?);
                }
            }
            ModelInputItem::Message { role, content } => {
                conversation_started = true;
                let role = match role {
                    ModelRole::User => "user",
                    ModelRole::Assistant => "assistant",
                    _ => unreachable!(),
                };
                messages.push(json!({"role":role,"content":content.iter().map(text_block)
                    .collect::<Result<Vec<_>, _>>()?}));
            }
            ModelInputItem::ToolObservation {
                model_call_id,
                result_json,
            } => {
                conversation_started = true;
                let content: Value = serde_json::from_str(result_json)
                    .map_err(|_| AnthropicAdapterError::InvalidRequest)?;
                messages.push(json!({"role":"user","content":[{"type":"tool_result",
                    "tool_use_id":model_call_id,"content":content}]}));
            }
            ModelInputItem::ReasoningReference { .. } => {
                return Err(AnthropicAdapterError::UnsupportedCapability)
            }
        }
    }
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            if tool.strict {
                return Err(AnthropicAdapterError::UnsupportedCapability);
            }
            let schema: Value = serde_json::from_str(&tool.input_schema_json)
                .map_err(|_| AnthropicAdapterError::InvalidRequest)?;
            Ok(json!({"name":tool.name,"description":tool.description,"input_schema":schema}))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut body = Map::new();
    body.insert("model".into(), json!(request.target_id.as_str()));
    body.insert("max_tokens".into(), json!(max_tokens));
    body.insert("messages".into(), Value::Array(messages));
    body.insert("stream".into(), json!(stream));
    if !system.is_empty() {
        body.insert("system".into(), Value::Array(system));
    }
    if !tools.is_empty() {
        body.insert("tools".into(), Value::Array(tools));
    }
    if !request.trace_metadata.is_empty() {
        if request.trace_metadata.len() != 1 || request.trace_metadata[0].0 != "user_id" {
            return Err(AnthropicAdapterError::UnsupportedCapability);
        }
        body.insert(
            "metadata".into(),
            json!({"user_id":request.trace_metadata[0].1}),
        );
    }
    Ok(Value::Object(body))
}

pub fn parse_response(bytes: &[u8]) -> Result<InvokeOutcome, AnthropicAdapterError> {
    let message: Value =
        serde_json::from_slice(bytes).map_err(|_| AnthropicAdapterError::InvalidJson)?;
    let items = parse_content(&message["content"])?;
    let usage = parse_usage(&message["usage"])?;
    Ok(InvokeOutcome::Completed {
        items,
        usage,
        stop_reason: parse_stop(
            message["stop_reason"]
                .as_str()
                .ok_or(AnthropicAdapterError::Invariant)?,
        )?,
    })
}

#[derive(Clone)]
enum Block {
    Text(String),
    Thinking(String, String),
    Tool(String, String, String),
}

pub fn parse_sse(bytes: &[u8]) -> Result<InvokeOutcome, AnthropicAdapterError> {
    let text = std::str::from_utf8(bytes).map_err(|_| AnthropicAdapterError::InvalidJson)?;
    let mut blocks = BTreeMap::<u64, (Block, bool)>::new();
    let mut usage = unknown_usage();
    let mut stop_reason = None;
    let mut started = false;
    let mut terminal = false;
    for line in text.lines().filter_map(|line| line.strip_prefix("data: ")) {
        if terminal {
            return Err(AnthropicAdapterError::Invariant);
        }
        let event: Value =
            serde_json::from_str(line).map_err(|_| AnthropicAdapterError::InvalidJson)?;
        match event["type"]
            .as_str()
            .ok_or(AnthropicAdapterError::Invariant)?
        {
            "message_start" => {
                if started {
                    return Err(AnthropicAdapterError::Invariant);
                }
                started = true;
                usage = parse_usage(&event["message"]["usage"])?;
            }
            "content_block_start" => {
                let index = index(&event)?;
                if blocks.contains_key(&index) {
                    return Err(AnthropicAdapterError::Invariant);
                }
                let value = &event["content_block"];
                let block = match value["type"]
                    .as_str()
                    .ok_or(AnthropicAdapterError::Invariant)?
                {
                    "text" => Block::Text(required(value, "text")?),
                    "thinking" => Block::Thinking(required(value, "thinking")?, String::new()),
                    "tool_use" => Block::Tool(
                        required(value, "id")?,
                        required(value, "name")?,
                        String::new(),
                    ),
                    _ => return Err(AnthropicAdapterError::UnsupportedCapability),
                };
                blocks.insert(index, (block, false));
            }
            "content_block_delta" => {
                let (block, stopped) = blocks
                    .get_mut(&index(&event)?)
                    .ok_or(AnthropicAdapterError::Invariant)?;
                if *stopped {
                    return Err(AnthropicAdapterError::Invariant);
                }
                let delta = &event["delta"];
                match (block, delta["type"].as_str()) {
                    (Block::Text(text), Some("text_delta")) => {
                        text.push_str(&required(delta, "text")?)
                    }
                    (Block::Thinking(text, _), Some("thinking_delta")) => {
                        text.push_str(&required(delta, "thinking")?)
                    }
                    (Block::Thinking(_, signature), Some("signature_delta")) => {
                        signature.push_str(&required(delta, "signature")?)
                    }
                    (Block::Tool(_, _, json), Some("input_json_delta")) => {
                        json.push_str(&required(delta, "partial_json")?)
                    }
                    _ => return Err(AnthropicAdapterError::Invariant),
                }
            }
            "content_block_stop" => {
                let (block, stopped) = blocks
                    .get_mut(&index(&event)?)
                    .ok_or(AnthropicAdapterError::Invariant)?;
                if *stopped {
                    return Err(AnthropicAdapterError::Invariant);
                }
                if let Block::Tool(_, _, value) = block {
                    serde_json::from_str::<Value>(value)
                        .map_err(|_| AnthropicAdapterError::Invariant)?;
                }
                *stopped = true;
            }
            "message_delta" => {
                stop_reason = Some(parse_stop(
                    required(&event["delta"], "stop_reason")?.as_str(),
                )?);
                let output = event["usage"]["output_tokens"]
                    .as_u64()
                    .ok_or(AnthropicAdapterError::Invariant)?;
                if let TokenCount::Known(previous) = usage.output_tokens {
                    if output < previous {
                        return Err(AnthropicAdapterError::Invariant);
                    }
                }
                usage.output_tokens = TokenCount::Known(output);
            }
            "message_stop" => {
                if blocks.values().any(|(_, stopped)| !stopped) || stop_reason.is_none() {
                    return Err(AnthropicAdapterError::Invariant);
                }
                terminal = true;
            }
            "ping" => {}
            "error" => return Err(AnthropicAdapterError::Invariant),
            _ => return Err(AnthropicAdapterError::UnsupportedCapability),
        }
    }
    let items = blocks
        .into_values()
        .map(|(block, _)| block_item(block))
        .collect::<Result<Vec<_>, _>>()?;
    if terminal {
        Ok(InvokeOutcome::Completed {
            items,
            usage,
            stop_reason: stop_reason.unwrap(),
        })
    } else {
        Ok(InvokeOutcome::Interrupted {
            kind: InterruptionKind::Transport,
            partial_items: items,
            usage,
        })
    }
}

fn parse_content(value: &Value) -> Result<Vec<ModelItem>, AnthropicAdapterError> {
    value
        .as_array()
        .ok_or(AnthropicAdapterError::Invariant)?
        .iter()
        .map(|value| {
            match value["type"]
                .as_str()
                .ok_or(AnthropicAdapterError::Invariant)?
            {
                "text" => Ok(ModelItem::Text {
                    text: required(value, "text")?,
                }),
                "thinking" => Ok(ModelItem::Reasoning {
                    content: ReasoningContent::ModelVisible(required(value, "thinking")?),
                }),
                "redacted_thinking" => Ok(ModelItem::Reasoning {
                    content: ReasoningContent::OpaqueReference(required(value, "data")?),
                }),
                "tool_use" => Ok(ModelItem::ToolIntent {
                    model_call_id: required(value, "id")?,
                    tool_name: required(value, "name")?,
                    arguments_json: value["input"].to_string(),
                }),
                _ => Err(AnthropicAdapterError::UnsupportedCapability),
            }
        })
        .collect()
}

fn block_item(block: Block) -> Result<ModelItem, AnthropicAdapterError> {
    Ok(match block {
        Block::Text(text) => ModelItem::Text { text },
        Block::Thinking(text, _) => ModelItem::Reasoning {
            content: ReasoningContent::ModelVisible(text),
        },
        Block::Tool(model_call_id, tool_name, arguments_json) => ModelItem::ToolIntent {
            model_call_id,
            tool_name,
            arguments_json,
        },
    })
}
fn parse_usage(value: &Value) -> Result<ModelUsage, AnthropicAdapterError> {
    let base = value["input_tokens"]
        .as_u64()
        .ok_or(AnthropicAdapterError::Invariant)?;
    let cache_write = value["cache_creation_input_tokens"].as_u64().unwrap_or(0);
    let cache_read = value["cache_read_input_tokens"].as_u64().unwrap_or(0);
    let input = base
        .checked_add(cache_write)
        .and_then(|v| v.checked_add(cache_read))
        .ok_or(AnthropicAdapterError::Invariant)?;
    let output = value["output_tokens"]
        .as_u64()
        .ok_or(AnthropicAdapterError::Invariant)?;
    Ok(ModelUsage {
        input_tokens: TokenCount::Known(input),
        output_tokens: TokenCount::Known(output),
        cache_read_tokens: Some(TokenCount::Known(cache_read)),
        cache_write_tokens: Some(TokenCount::Known(cache_write)),
        source: UsageSource::ProviderReported,
    })
}
fn parse_stop(value: &str) -> Result<ModelStopReason, AnthropicAdapterError> {
    Ok(match value {
        "end_turn" => ModelStopReason::EndTurn,
        "tool_use" => ModelStopReason::ToolUse,
        "stop_sequence" => ModelStopReason::StopSequence,
        "pause_turn" => ModelStopReason::PauseTurn,
        "refusal" => ModelStopReason::Refusal,
        "max_tokens" | "model_context_window_exceeded" => {
            return Err(AnthropicAdapterError::UnsupportedCapability)
        }
        _ => ModelStopReason::Other(value.into()),
    })
}
fn text_block(value: &ModelInputContent) -> Result<Value, AnthropicAdapterError> {
    match value {
        ModelInputContent::Text(text) => Ok(json!({"type":"text","text":text})),
        _ => Err(AnthropicAdapterError::UnsupportedCapability),
    }
}
fn required(value: &Value, key: &str) -> Result<String, AnthropicAdapterError> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or(AnthropicAdapterError::Invariant)
}
fn index(value: &Value) -> Result<u64, AnthropicAdapterError> {
    value["index"]
        .as_u64()
        .ok_or(AnthropicAdapterError::Invariant)
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
