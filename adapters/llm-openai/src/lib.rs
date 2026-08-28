//! Strict OpenAI Responses wire codec. HTTP ownership remains in Runtime.

#![forbid(unsafe_code)]

use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelFuture, ModelInputContent,
    ModelInputItem, ModelItem, ModelObserver, ModelOutputKind, ModelPort, ModelPortFailure,
    ModelRequest, ModelRole, ModelStopReason, ModelStreamEvent, ModelUsage, ObserverDecision,
    ReasoningContent, RejectionKind, TextMode, TokenCount, UnavailableKind, UsageSource,
};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiAdapterError {
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
    Connection,
    Timeout,
}

pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait OpenAiTransport: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: HttpRequestDescriptor,
        cancellation: &'a dyn ModelCancellation,
    ) -> TransportFuture<'a, Result<HttpResponseDescriptor, TransportFailure>>;

    fn wait<'a>(&'a self, delay: Duration) -> TransportFuture<'a, ()>;
}

pub struct OpenAiModelPort<T> {
    transport: T,
    max_attempts: u32,
}

impl<T> OpenAiModelPort<T> {
    pub const fn new(transport: T, max_attempts: u32) -> Self {
        Self {
            transport,
            max_attempts,
        }
    }
}

impl<T: OpenAiTransport> ModelPort for OpenAiModelPort<T> {
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
                    Err(_) if attempt < self.max_attempts => {
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
) -> Result<HttpRequestDescriptor, OpenAiAdapterError> {
    let body = serde_json::to_vec(&render_request(request, stream)?)
        .map_err(|_| OpenAiAdapterError::Invariant)?;
    Ok(HttpRequestDescriptor {
        method: "POST",
        path: "/v1/responses",
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
) -> Result<HttpErrorAction, OpenAiAdapterError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| OpenAiAdapterError::InvalidJson)?;
    let error = value
        .get("error")
        .and_then(Value::as_object)
        .ok_or(OpenAiAdapterError::Invariant)?;
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let evidence = sanitized_evidence(code, kind);
    if code == "context_length_exceeded" {
        return Ok(HttpErrorAction::Terminal(InvokeOutcome::Rejected {
            kind: RejectionKind::ContextOverflow,
            sanitized_evidence: evidence,
        }));
    }
    if matches!(status, 401 | 403) || code == "invalid_api_key" {
        return Ok(HttpErrorAction::Terminal(InvokeOutcome::Rejected {
            kind: RejectionKind::Authentication,
            sanitized_evidence: evidence,
        }));
    }
    let unavailable = match status {
        429 => Some(UnavailableKind::RateLimited),
        500..=599 => Some(UnavailableKind::ModelUnavailable),
        _ => None,
    }
    .ok_or(OpenAiAdapterError::UnsupportedCapability)?;
    let delay = retry_after.and_then(|value| parse_retry_after(value, now));
    if !exhausted {
        return Ok(HttpErrorAction::Retry { retry_after: delay });
    }
    Ok(HttpErrorAction::Terminal(InvokeOutcome::Unavailable {
        kind: unavailable,
        retry_after: delay,
    }))
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
    let status = response["status"]
        .as_str()
        .ok_or(OpenAiAdapterError::Invariant)?;
    if !matches!(status, "completed" | "incomplete") {
        return Err(OpenAiAdapterError::Invariant);
    }
    let items = parse_items(&response["output"])?;
    let usage = parse_usage(&response["usage"])?;
    if status == "incomplete" {
        match response["incomplete_details"]["reason"].as_str() {
            Some("max_output_tokens") => {}
            Some("content_filter") => {
                return Ok(InvokeOutcome::Rejected {
                    kind: RejectionKind::ContentPolicy,
                    sanitized_evidence: "incomplete:content_filter".into(),
                })
            }
            _ => return Err(OpenAiAdapterError::UnsupportedCapability),
        }
        return Ok(InvokeOutcome::Interrupted {
            kind: InterruptionKind::OutputLimit,
            partial_items: items,
            usage,
        });
    }
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum StreamField {
    OutputText(u64),
    Refusal(u64),
    FunctionArguments,
    ReasoningSummary(u64),
    ReasoningText(u64),
}

#[derive(Clone, Debug)]
struct StartedItem {
    id: String,
    kind: String,
    call_id: Option<String>,
    name: Option<String>,
}

pub fn parse_sse(bytes: &[u8]) -> Result<InvokeOutcome, OpenAiAdapterError> {
    let text = std::str::from_utf8(bytes).map_err(|_| OpenAiAdapterError::InvalidJson)?;
    let mut previous = None;
    let mut assembled = BTreeMap::<(u64, StreamField), String>::new();
    let mut started = BTreeMap::<u64, StartedItem>::new();
    let mut completed = BTreeSet::new();
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
            "response.output_item.added" => {
                let index = output_index(&event)?;
                let item = &event["item"];
                let state = StartedItem {
                    id: required_text(item, "id")?,
                    kind: required_text(item, "type")?,
                    call_id: item["call_id"].as_str().map(str::to_owned),
                    name: item["name"].as_str().map(str::to_owned),
                };
                if started.insert(index, state).is_some() {
                    return Err(OpenAiAdapterError::Invariant);
                }
            }
            "response.output_text.delta"
            | "response.refusal.delta"
            | "response.function_call_arguments.delta"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta" => {
                let index = output_index(&event)?;
                let field = stream_field(&event)?;
                require_started(&started, &completed, index, &event, &field)?;
                assembled
                    .entry((index, field))
                    .or_default()
                    .push_str(required_text(&event, "delta")?.as_str());
            }
            "response.output_text.done"
            | "response.refusal.done"
            | "response.function_call_arguments.done"
            | "response.reasoning_summary_text.done"
            | "response.reasoning_text.done" => {
                let index = output_index(&event)?;
                let field = stream_field(&event)?;
                require_started(&started, &completed, index, &event, &field)?;
                let final_value = match field {
                    StreamField::Refusal(_) => event["refusal"].as_str(),
                    StreamField::FunctionArguments => event["arguments"].as_str(),
                    _ => event["text"].as_str(),
                };
                if assembled.get(&(index, field)).map(String::as_str) != final_value {
                    return Err(OpenAiAdapterError::Invariant);
                }
            }
            "response.output_item.done" => {
                let index = output_index(&event)?;
                let state = started.get(&index).ok_or(OpenAiAdapterError::Invariant)?;
                if state.id != required_text(&event["item"], "id")?
                    || state.kind != required_text(&event["item"], "type")?
                    || !completed.insert(index)
                {
                    return Err(OpenAiAdapterError::Invariant);
                }
                verify_item_assembled(&assembled, index, &event["item"])?;
            }
            "response.content_part.added" => verify_part_event(&started, &event, false)?,
            "response.content_part.done" => {
                verify_part_event(&started, &event, true)?;
                verify_part_done(&assembled, &event)?;
            }
            "response.reasoning_summary_part.added" => {
                verify_summary_part_event(&started, &event, false)?
            }
            "response.reasoning_summary_part.done" => {
                verify_summary_part_event(&started, &event, true)?;
                verify_part_done(&assembled, &event)?;
            }
            "response.completed" => {
                if started.keys().copied().collect::<BTreeSet<_>>() != completed {
                    return Err(OpenAiAdapterError::Invariant);
                }
                verify_assembled(&assembled, &event["response"])?;
                terminal = Some(parse_response(event["response"].to_string().as_bytes())?)
            }
            "response.incomplete" => {
                verify_assembled(&assembled, &event["response"])?;
                let response = &event["response"];
                if response["incomplete_details"]["reason"] != "max_output_tokens" {
                    return Err(OpenAiAdapterError::UnsupportedCapability);
                }
                terminal = Some(InvokeOutcome::Interrupted {
                    kind: InterruptionKind::OutputLimit,
                    partial_items: parse_items(&response["output"])?,
                    usage: parse_usage(&response["usage"])?,
                });
            }
            "response.failed" => return Err(OpenAiAdapterError::Invariant),
            "response.created"
            | "response.in_progress"
            | "response.queued"
            | "response.output_text.annotation.added" => {}
            _ => return Err(OpenAiAdapterError::UnsupportedCapability),
        }
    }
    if let Some(outcome) = terminal {
        return Ok(outcome);
    }
    Ok(InvokeOutcome::Interrupted {
        kind: InterruptionKind::Transport,
        partial_items: assembled_items(assembled, &started),
        usage: unknown_usage(),
    })
}

fn verify_assembled(
    assembled: &BTreeMap<(u64, StreamField), String>,
    response: &Value,
) -> Result<(), OpenAiAdapterError> {
    let output = response["output"]
        .as_array()
        .ok_or(OpenAiAdapterError::Invariant)?;
    for ((index, field), text) in assembled {
        let item = output
            .get(usize::try_from(*index).map_err(|_| OpenAiAdapterError::Invariant)?)
            .ok_or(OpenAiAdapterError::Invariant)?;
        let final_text = item_field_text(item, field)?;
        if text != final_text {
            return Err(OpenAiAdapterError::Invariant);
        }
    }
    Ok(())
}

fn verify_item_assembled(
    assembled: &BTreeMap<(u64, StreamField), String>,
    index: u64,
    item: &Value,
) -> Result<(), OpenAiAdapterError> {
    for ((_, field), text) in assembled
        .iter()
        .filter(|((item_index, _), _)| *item_index == index)
    {
        if text != item_field_text(item, field)? {
            return Err(OpenAiAdapterError::Invariant);
        }
    }
    Ok(())
}

fn item_field_text<'a>(
    item: &'a Value,
    field: &StreamField,
) -> Result<&'a str, OpenAiAdapterError> {
    Ok(match field {
        StreamField::OutputText(content_index) => {
            indexed_text(item, "content", *content_index, "text")?
        }
        StreamField::Refusal(content_index) => {
            indexed_text(item, "content", *content_index, "refusal")?
        }
        StreamField::FunctionArguments => item["arguments"]
            .as_str()
            .ok_or(OpenAiAdapterError::Invariant)?,
        StreamField::ReasoningSummary(summary_index) => {
            indexed_text(item, "summary", *summary_index, "text")?
        }
        StreamField::ReasoningText(content_index) => {
            indexed_text(item, "content", *content_index, "text")?
        }
    })
}

fn verify_part_event(
    started: &BTreeMap<u64, StartedItem>,
    event: &Value,
    done: bool,
) -> Result<(), OpenAiAdapterError> {
    let index = output_index(event)?;
    let state = started.get(&index).ok_or(OpenAiAdapterError::Invariant)?;
    if state.id != required_text(event, "item_id")? || state.kind != "message" {
        return Err(OpenAiAdapterError::Invariant);
    }
    let part = &event["part"];
    let kind = required_text(part, "type")?;
    if !matches!(kind.as_str(), "output_text" | "refusal" | "reasoning_text") {
        return Err(OpenAiAdapterError::UnsupportedCapability);
    }
    if done && !part.is_object() {
        return Err(OpenAiAdapterError::Invariant);
    }
    Ok(())
}

fn verify_summary_part_event(
    started: &BTreeMap<u64, StartedItem>,
    event: &Value,
    done: bool,
) -> Result<(), OpenAiAdapterError> {
    let index = output_index(event)?;
    let state = started.get(&index).ok_or(OpenAiAdapterError::Invariant)?;
    if state.id != required_text(event, "item_id")? || state.kind != "reasoning" {
        return Err(OpenAiAdapterError::Invariant);
    }
    let part = &event["part"];
    if required_text(part, "type")? != "summary_text" || (done && !part.is_object()) {
        return Err(OpenAiAdapterError::Invariant);
    }
    Ok(())
}

fn verify_part_done(
    assembled: &BTreeMap<(u64, StreamField), String>,
    event: &Value,
) -> Result<(), OpenAiAdapterError> {
    let index = output_index(event)?;
    let part = &event["part"];
    let field = match required_text(part, "type")?.as_str() {
        "output_text" => StreamField::OutputText(content_index(event)?),
        "refusal" => StreamField::Refusal(content_index(event)?),
        "reasoning_text" => StreamField::ReasoningText(content_index(event)?),
        "summary_text" => StreamField::ReasoningSummary(required_u64(event, "summary_index")?),
        _ => return Err(OpenAiAdapterError::UnsupportedCapability),
    };
    let key = match field {
        StreamField::Refusal(_) => "refusal",
        _ => "text",
    };
    if let Some(value) = assembled.get(&(index, field)) {
        if part[key].as_str() != Some(value.as_str()) {
            return Err(OpenAiAdapterError::Invariant);
        }
    }
    Ok(())
}

fn stream_field(event: &Value) -> Result<StreamField, OpenAiAdapterError> {
    let kind = required_text(event, "type")?;
    Ok(match kind.as_str() {
        "response.output_text.delta" | "response.output_text.done" => {
            StreamField::OutputText(content_index(event)?)
        }
        "response.refusal.delta" | "response.refusal.done" => {
            StreamField::Refusal(content_index(event)?)
        }
        "response.function_call_arguments.delta" | "response.function_call_arguments.done" => {
            StreamField::FunctionArguments
        }
        "response.reasoning_summary_text.delta" | "response.reasoning_summary_text.done" => {
            StreamField::ReasoningSummary(required_u64(event, "summary_index")?)
        }
        "response.reasoning_text.delta" | "response.reasoning_text.done" => {
            StreamField::ReasoningText(content_index(event)?)
        }
        _ => return Err(OpenAiAdapterError::Invariant),
    })
}

fn require_started(
    started: &BTreeMap<u64, StartedItem>,
    completed: &BTreeSet<u64>,
    index: u64,
    event: &Value,
    field: &StreamField,
) -> Result<(), OpenAiAdapterError> {
    let state = started.get(&index).ok_or(OpenAiAdapterError::Invariant)?;
    let expected_kind = match field {
        StreamField::OutputText(_) | StreamField::Refusal(_) => "message",
        StreamField::FunctionArguments => "function_call",
        StreamField::ReasoningSummary(_) | StreamField::ReasoningText(_) => "reasoning",
    };
    if completed.contains(&index)
        || state.kind != expected_kind
        || state.id != required_text(event, "item_id")?
    {
        return Err(OpenAiAdapterError::Invariant);
    }
    Ok(())
}

fn assembled_items(
    assembled: BTreeMap<(u64, StreamField), String>,
    started: &BTreeMap<u64, StartedItem>,
) -> Vec<ModelItem> {
    assembled
        .into_iter()
        .map(|((index, field), value)| match field {
            StreamField::OutputText(_) => ModelItem::Text { text: value },
            StreamField::Refusal(_) => ModelItem::Refusal { text: value },
            StreamField::FunctionArguments => {
                let state = &started[&index];
                ModelItem::ToolIntent {
                    model_call_id: state.call_id.clone().unwrap_or_default(),
                    tool_name: state.name.clone().unwrap_or_default(),
                    arguments_json: value,
                }
            }
            StreamField::ReasoningSummary(_) | StreamField::ReasoningText(_) => {
                ModelItem::Reasoning {
                    content: ReasoningContent::ModelVisible(value),
                }
            }
        })
        .collect()
}

fn indexed_text<'a>(
    item: &'a Value,
    list: &str,
    index: u64,
    key: &str,
) -> Result<&'a str, OpenAiAdapterError> {
    item[list]
        .as_array()
        .and_then(|values| values.get(index as usize))
        .and_then(|value| value[key].as_str())
        .ok_or(OpenAiAdapterError::Invariant)
}

fn required_u64(value: &Value, key: &str) -> Result<u64, OpenAiAdapterError> {
    value[key].as_u64().ok_or(OpenAiAdapterError::Invariant)
}
fn output_index(value: &Value) -> Result<u64, OpenAiAdapterError> {
    required_u64(value, "output_index")
}
fn content_index(value: &Value) -> Result<u64, OpenAiAdapterError> {
    required_u64(value, "content_index")
}

fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    httpdate::parse_http_date(value)
        .ok()
        .and_then(|deadline| deadline.duration_since(now).ok())
}

fn sanitized_evidence(code: &str, kind: &str) -> String {
    format!("{kind}:{code}").chars().take(128).collect()
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
                for summary in value["summary"].as_array().into_iter().flatten() {
                    items.push(ModelItem::Reasoning {
                        content: ReasoningContent::ModelVisible(required_text(summary, "text")?),
                    });
                }
                for content in value["content"].as_array().into_iter().flatten() {
                    items.push(ModelItem::Reasoning {
                        content: ReasoningContent::ModelVisible(required_text(content, "text")?),
                    });
                }
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

const fn port_failure(error: OpenAiAdapterError) -> ModelPortFailure {
    match error {
        OpenAiAdapterError::InvalidRequest => ModelPortFailure::InvalidRequest,
        OpenAiAdapterError::UnsupportedCapability => ModelPortFailure::UnsupportedCapability,
        OpenAiAdapterError::InvalidJson | OpenAiAdapterError::Invariant => {
            ModelPortFailure::AdapterInvariant
        }
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
