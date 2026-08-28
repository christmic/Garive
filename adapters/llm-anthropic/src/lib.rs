//! Strict Anthropic Messages wire codec. HTTP headers and secrets remain in Runtime.

#![forbid(unsafe_code)]

use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelFuture, ModelInputContent,
    ModelInputItem, ModelItem, ModelObserver, ModelOutputKind, ModelPort, ModelPortFailure,
    ModelRequest, ModelRole, ModelStopReason, ModelStreamEvent, ModelUsage, ObserverDecision,
    ReasoningContent, RejectionKind, TextMode, TokenCount, UnavailableKind, UsageSource,
};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnthropicAdapterError {
    InvalidRequest,
    UnsupportedCapability,
    InvalidJson,
    Invariant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HttpErrorAction {
    Retry { retry_after: Option<Duration> },
    Terminal(InvokeOutcome),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequestDescriptor {
    pub method: &'static str,
    pub path: &'static str,
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: Vec<u8>,
}

pub struct HttpResponseDescriptor {
    pub status: u16,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportFailure {
    BeforeDispatch,
    Ambiguous,
}

pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait AnthropicTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: HttpRequestDescriptor,
        cancellation: &'a dyn ModelCancellation,
    ) -> TransportFuture<'a, Result<HttpResponseDescriptor, TransportFailure>>;

    fn wait<'a>(&'a self, delay: Duration) -> TransportFuture<'a, ()>;
}

pub struct AnthropicModelPort<T> {
    transport: T,
    max_attempts: u32,
}

impl<T> AnthropicModelPort<T> {
    pub const fn new(transport: T, max_attempts: u32) -> Self {
        Self {
            transport,
            max_attempts,
        }
    }
}

impl<T: AnthropicTransport> ModelPort for AnthropicModelPort<T> {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            if self.max_attempts == 0 {
                return Err(ModelPortFailure::InvalidRequest);
            }
            if cancellation.is_cancelled() {
                return Ok(cancelled_outcome(None));
            }
            for attempt in 1..=self.max_attempts {
                let descriptor = render_http_request(request, true).map_err(port_failure)?;
                let response = self.transport.execute(descriptor, cancellation).await;
                if cancellation.is_cancelled() {
                    return Ok(cancelled_outcome(None));
                }
                let response = match response {
                    Ok(value) => value,
                    Err(TransportFailure::BeforeDispatch) if attempt < self.max_attempts => {
                        self.transport.wait(Duration::ZERO).await;
                        continue;
                    }
                    Err(_) => {
                        return Ok(InvokeOutcome::Interrupted {
                            kind: InterruptionKind::Transport,
                            partial_items: Vec::new(),
                            usage: unknown_usage(),
                        })
                    }
                };
                if (200..300).contains(&response.status) {
                    let outcome = parse_sse(&response.body).map_err(port_failure)?;
                    return if cancellation.is_cancelled() {
                        Ok(cancelled_outcome(Some(outcome)))
                    } else {
                        Ok(notify_outcome(outcome, observer, cancellation))
                    };
                }
                match classify_http_error(
                    response.status,
                    response.retry_after.as_deref(),
                    &response.body,
                    attempt == self.max_attempts,
                    SystemTime::now(),
                )
                .map_err(port_failure)?
                {
                    HttpErrorAction::Retry { retry_after } => {
                        self.transport.wait(retry_after.unwrap_or_default()).await
                    }
                    HttpErrorAction::Terminal(outcome) => return Ok(outcome),
                }
            }
            Err(ModelPortFailure::AdapterInvariant)
        })
    }
}

pub fn render_http_request(
    request: &ModelRequest,
    stream: bool,
) -> Result<HttpRequestDescriptor, AnthropicAdapterError> {
    let body = serde_json::to_vec(&render_request(request, stream)?)
        .map_err(|_| AnthropicAdapterError::Invariant)?;
    Ok(HttpRequestDescriptor {
        method: "POST",
        path: "/v1/messages",
        headers: vec![
            ("content-type", "application/json"),
            (
                "accept",
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            ),
            ("anthropic-version", "2023-06-01"),
        ],
        body,
    })
}

pub fn classify_http_error(
    status: u16,
    retry_after: Option<&str>,
    body: &[u8],
    exhausted: bool,
    now: SystemTime,
) -> Result<HttpErrorAction, AnthropicAdapterError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| AnthropicAdapterError::InvalidJson)?;
    let error = value
        .get("error")
        .and_then(Value::as_object)
        .ok_or(AnthropicAdapterError::Invariant)?;
    let kind = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let evidence = format!("type:{}", bounded(kind, 64));
    if kind == "invalid_request_error"
        && (message.contains("prompt is too long") || message.contains("context window"))
    {
        return Ok(HttpErrorAction::Terminal(InvokeOutcome::Rejected {
            kind: RejectionKind::ContextOverflow,
            sanitized_evidence: evidence,
        }));
    }
    if matches!(status, 401 | 403) || matches!(kind, "authentication_error" | "permission_error") {
        return Ok(HttpErrorAction::Terminal(InvokeOutcome::Rejected {
            kind: RejectionKind::Authentication,
            sanitized_evidence: evidence,
        }));
    }
    let unavailable = if status == 429 || kind == "rate_limit_error" {
        Some(UnavailableKind::RateLimited)
    } else if matches!(status, 500 | 503 | 504 | 529)
        || matches!(kind, "api_error" | "overloaded_error")
    {
        Some(UnavailableKind::ModelUnavailable)
    } else {
        None
    }
    .ok_or(AnthropicAdapterError::UnsupportedCapability)?;
    let delay = retry_after.and_then(|value| parse_retry_after(value, now));
    if !exhausted {
        return Ok(HttpErrorAction::Retry { retry_after: delay });
    }
    Ok(HttpErrorAction::Terminal(InvokeOutcome::Unavailable {
        kind: unavailable,
        retry_after: delay,
    }))
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
                let _: Value = serde_json::from_str(result_json)
                    .map_err(|_| AnthropicAdapterError::InvalidRequest)?;
                messages.push(json!({"role":"user","content":[{"type":"tool_result",
                    "tool_use_id":model_call_id,"content":result_json}]}));
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
    match parse_stop(required(&message, "stop_reason")?.as_str())? {
        ParsedStop::Completed(stop_reason) => Ok(InvokeOutcome::Completed {
            items,
            usage,
            stop_reason,
        }),
        ParsedStop::OutputLimit => Ok(InvokeOutcome::Interrupted {
            kind: InterruptionKind::OutputLimit,
            partial_items: items,
            usage,
        }),
    }
}

#[derive(Clone)]
enum Block {
    Text(String),
    Thinking(String, String),
    RedactedThinking(String),
    Tool(String, String, String),
}

enum ParsedStop {
    Completed(ModelStopReason),
    OutputLimit,
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
                    "redacted_thinking" => Block::RedactedThinking(required(value, "data")?),
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
            "error" => {
                let items: Vec<ModelItem> = blocks
                    .values()
                    .cloned()
                    .flat_map(|(block, _)| block_items(block))
                    .collect();
                if !items.is_empty() {
                    return Ok(InvokeOutcome::Interrupted {
                        kind: InterruptionKind::Transport,
                        partial_items: items,
                        usage,
                    });
                }
                return classify_stream_error(&event);
            }
            _ => return Err(AnthropicAdapterError::UnsupportedCapability),
        }
    }
    let items: Vec<ModelItem> = blocks
        .into_values()
        .flat_map(|(block, _)| block_items(block))
        .collect();
    if terminal {
        match stop_reason.unwrap() {
            ParsedStop::Completed(stop_reason) => Ok(InvokeOutcome::Completed {
                items,
                usage,
                stop_reason,
            }),
            ParsedStop::OutputLimit => Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::OutputLimit,
                partial_items: items,
                usage,
            }),
        }
    } else {
        Ok(InvokeOutcome::Interrupted {
            kind: InterruptionKind::Transport,
            partial_items: items,
            usage,
        })
    }
}

fn parse_content(value: &Value) -> Result<Vec<ModelItem>, AnthropicAdapterError> {
    let groups = value
        .as_array()
        .ok_or(AnthropicAdapterError::Invariant)?
        .iter()
        .map(|value| {
            match value["type"]
                .as_str()
                .ok_or(AnthropicAdapterError::Invariant)?
            {
                "text" => Ok(vec![ModelItem::Text {
                    text: required(value, "text")?,
                }]),
                "thinking" => {
                    let mut items = vec![ModelItem::Reasoning {
                        content: ReasoningContent::ModelVisible(required(value, "thinking")?),
                    }];
                    if let Some(signature) = value["signature"].as_str() {
                        items.push(ModelItem::Reasoning {
                            content: ReasoningContent::OpaqueReference(signature.into()),
                        });
                    }
                    Ok(items)
                }
                "redacted_thinking" => Ok(vec![ModelItem::Reasoning {
                    content: ReasoningContent::OpaqueReference(required(value, "data")?),
                }]),
                "tool_use" => Ok(vec![ModelItem::ToolIntent {
                    model_call_id: required(value, "id")?,
                    tool_name: required(value, "name")?,
                    arguments_json: value["input"].to_string(),
                }]),
                _ => Err(AnthropicAdapterError::UnsupportedCapability),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(groups.into_iter().flatten().collect())
}

fn block_items(block: Block) -> Vec<ModelItem> {
    match block {
        Block::Text(text) => vec![ModelItem::Text { text }],
        Block::Thinking(text, signature) => {
            let mut items = vec![ModelItem::Reasoning {
                content: ReasoningContent::ModelVisible(text),
            }];
            if !signature.is_empty() {
                items.push(ModelItem::Reasoning {
                    content: ReasoningContent::OpaqueReference(signature),
                });
            }
            items
        }
        Block::RedactedThinking(data) => vec![ModelItem::Reasoning {
            content: ReasoningContent::OpaqueReference(data),
        }],
        Block::Tool(model_call_id, tool_name, arguments_json) => vec![ModelItem::ToolIntent {
            model_call_id,
            tool_name,
            arguments_json,
        }],
    }
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
fn parse_stop(value: &str) -> Result<ParsedStop, AnthropicAdapterError> {
    Ok(match value {
        "end_turn" => ParsedStop::Completed(ModelStopReason::EndTurn),
        "tool_use" => ParsedStop::Completed(ModelStopReason::ToolUse),
        "stop_sequence" => ParsedStop::Completed(ModelStopReason::StopSequence),
        "pause_turn" => ParsedStop::Completed(ModelStopReason::PauseTurn),
        "refusal" => ParsedStop::Completed(ModelStopReason::Refusal),
        "max_tokens" | "model_context_window_exceeded" => ParsedStop::OutputLimit,
        _ => ParsedStop::Completed(ModelStopReason::Other(value.into())),
    })
}

fn classify_stream_error(event: &Value) -> Result<InvokeOutcome, AnthropicAdapterError> {
    let kind = event["error"]["type"]
        .as_str()
        .ok_or(AnthropicAdapterError::Invariant)?;
    let evidence = format!("type:{}", bounded(kind, 64));
    Ok(match kind {
        "authentication_error" | "permission_error" => InvokeOutcome::Rejected {
            kind: RejectionKind::Authentication,
            sanitized_evidence: evidence,
        },
        "rate_limit_error" => InvokeOutcome::Unavailable {
            kind: UnavailableKind::RateLimited,
            retry_after: None,
        },
        "api_error" | "overloaded_error" => InvokeOutcome::Unavailable {
            kind: UnavailableKind::ModelUnavailable,
            retry_after: None,
        },
        _ => return Err(AnthropicAdapterError::UnsupportedCapability),
    })
}

fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    httpdate::parse_http_date(value)
        .ok()
        .and_then(|deadline| deadline.duration_since(now).ok())
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
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

fn cancelled_outcome(previous: Option<InvokeOutcome>) -> InvokeOutcome {
    let (partial_items, usage) = match previous {
        Some(InvokeOutcome::Completed { items, usage, .. }) => (items, usage),
        Some(InvokeOutcome::Interrupted {
            partial_items,
            usage,
            ..
        }) => (partial_items, usage),
        _ => (Vec::new(), unknown_usage()),
    };
    InvokeOutcome::Interrupted {
        kind: InterruptionKind::Cancelled,
        partial_items,
        usage,
    }
}

fn notify_outcome(
    outcome: InvokeOutcome,
    observer: &mut dyn ModelObserver,
    cancellation: &dyn ModelCancellation,
) -> InvokeOutcome {
    let (items, usage) = match &outcome {
        InvokeOutcome::Completed { items, usage, .. } => (items.clone(), *usage),
        InvokeOutcome::Interrupted {
            partial_items,
            usage,
            ..
        } => (partial_items.clone(), *usage),
        _ => return outcome,
    };
    let mut observed = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let Ok(output_index) = u32::try_from(index) else {
            return outcome;
        };
        if cancellation.is_cancelled()
            || observer.observe(&ModelStreamEvent::OutputItemStarted {
                output_index,
                kind: output_kind(&item),
            }) == ObserverDecision::Cancel
        {
            return cancelled_with(observed, usage);
        }
        observed.push(item.clone());
        if observer.observe(&ModelStreamEvent::OutputItemCompleted { output_index, item })
            == ObserverDecision::Cancel
        {
            return cancelled_with(observed, usage);
        }
    }
    if cancellation.is_cancelled()
        || observer.observe(&ModelStreamEvent::UsageUpdated { usage }) == ObserverDecision::Cancel
    {
        return cancelled_with(observed, usage);
    }
    outcome
}

fn cancelled_with(partial_items: Vec<ModelItem>, usage: ModelUsage) -> InvokeOutcome {
    InvokeOutcome::Interrupted {
        kind: InterruptionKind::Cancelled,
        partial_items,
        usage,
    }
}

fn output_kind(item: &ModelItem) -> ModelOutputKind {
    match item {
        ModelItem::Text { .. } => ModelOutputKind::Text,
        ModelItem::Refusal { .. } => ModelOutputKind::Refusal,
        ModelItem::Reasoning { .. } => ModelOutputKind::Reasoning,
        ModelItem::ToolIntent { model_call_id, .. } => ModelOutputKind::ToolIntent {
            model_call_id: model_call_id.clone(),
        },
        ModelItem::ToolObservation { .. } => ModelOutputKind::ToolObservation,
        ModelItem::MediaReference { .. } => ModelOutputKind::MediaReference,
    }
}

const fn port_failure(error: AnthropicAdapterError) -> ModelPortFailure {
    match error {
        AnthropicAdapterError::InvalidRequest => ModelPortFailure::InvalidRequest,
        AnthropicAdapterError::UnsupportedCapability => ModelPortFailure::UnsupportedCapability,
        AnthropicAdapterError::InvalidJson | AnthropicAdapterError::Invariant => {
            ModelPortFailure::AdapterInvariant
        }
    }
}
