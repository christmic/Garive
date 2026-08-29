use std::collections::BTreeMap;

use garive_anthropic_messages as messages;
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelItem, ModelOutputKind, ModelStopReason, ModelStreamEvent,
    ModelUsage, ReasoningContent, TokenCount, UsageSource,
};
use garive_openai_responses as responses;
use serde_json::{Map, Value};

use crate::{normalize_responses, CompatibleProviderError};

/// Semantic events and optional terminal produced by one protocol stream event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamMapping {
    /// Ordered neutral progress events.
    pub events: Vec<ModelStreamEvent>,
    /// Final neutral terminal when this protocol event completed the exchange.
    pub terminal: Option<InvokeOutcome>,
}

impl StreamMapping {
    fn events(events: Vec<ModelStreamEvent>) -> Self {
        Self {
            events,
            terminal: None,
        }
    }

    fn terminal(terminal: InvokeOutcome) -> Self {
        Self {
            events: Vec::new(),
            terminal: Some(terminal),
        }
    }

    fn empty() -> Self {
        Self::events(Vec::new())
    }
}

/// Stable stream semantic mapping failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamMappingError {
    /// Protocol extension is not admitted by the portable provider.
    Provider(CompatibleProviderError),
    /// A required typed field was absent or contradicted prior state.
    ProtocolInvariant,
}

impl From<CompatibleProviderError> for StreamMappingError {
    fn from(value: CompatibleProviderError) -> Self {
        Self::Provider(value)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ResponsesKey {
    Item(u64),
    Content(u64, u64),
}

#[derive(Clone, Debug)]
struct ResponsesOpen {
    neutral_index: u32,
    kind: ModelOutputKind,
}

/// Stateful semantic mapper for adapter-validated Responses stream events.
#[derive(Clone, Debug)]
pub struct ResponsesStreamMapper {
    reasoning_visibility: bool,
    next_index: u32,
    open: BTreeMap<ResponsesKey, ResponsesOpen>,
}

impl ResponsesStreamMapper {
    /// Creates a mapper with the request's frozen reasoning visibility.
    pub fn new(reasoning_visibility: bool) -> Self {
        Self {
            reasoning_visibility,
            next_index: 0,
            open: BTreeMap::new(),
        }
    }

    /// Converts one event already validated by the protocol adapter.
    pub fn accept(
        &mut self,
        event: &responses::ResponseStreamEvent,
    ) -> Result<StreamMapping, StreamMappingError> {
        let responses::ResponseStreamEvent::Portable { kind, object, .. } = event else {
            return Err(CompatibleProviderError::UnsupportedExtension.into());
        };
        use responses::PortableEventKind as Kind;
        match kind {
            Kind::Created | Kind::Queued | Kind::InProgress | Kind::OutputTextAnnotationAdded => {
                Ok(StreamMapping::empty())
            }
            Kind::OutputItemAdded => self.responses_item_start(object),
            Kind::ContentPartAdded => self.responses_content_start(object),
            Kind::OutputTextDelta => self.responses_delta(object, "delta", Delta::Text),
            Kind::RefusalDelta => self.responses_delta(object, "delta", Delta::Refusal),
            Kind::FunctionArgumentsDelta => {
                self.responses_delta(object, "delta", Delta::ToolArguments)
            }
            Kind::ReasoningSummaryTextDelta | Kind::ReasoningTextDelta => {
                if self.reasoning_visibility {
                    self.responses_delta(object, "delta", Delta::Reasoning)
                } else {
                    Ok(StreamMapping::empty())
                }
            }
            Kind::ContentPartDone => self.responses_content_done(object),
            Kind::OutputItemDone => self.responses_item_done(object),
            Kind::Completed | Kind::Incomplete => {
                let response: responses::Response = serde_json::from_value(
                    object
                        .get("response")
                        .cloned()
                        .ok_or(StreamMappingError::ProtocolInvariant)?,
                )
                .map_err(|_| StreamMappingError::ProtocolInvariant)?;
                Ok(StreamMapping::terminal(normalize_responses(
                    &response,
                    self.reasoning_visibility,
                )?))
            }
            Kind::Failed | Kind::Error => Err(StreamMappingError::Provider(
                CompatibleProviderError::UnclassifiedProtocolError,
            )),
            Kind::OutputTextDone
            | Kind::RefusalDone
            | Kind::FunctionArgumentsDone
            | Kind::ReasoningSummaryPartAdded
            | Kind::ReasoningSummaryPartDone
            | Kind::ReasoningSummaryTextDone
            | Kind::ReasoningTextDone => Ok(StreamMapping::empty()),
        }
    }

    fn responses_item_start(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<StreamMapping, StreamMappingError> {
        let output_index = required_u64(object, "output_index")?;
        let item: responses::ResponseOutputItem = serde_json::from_value(required(object, "item")?)
            .map_err(|_| StreamMappingError::ProtocolInvariant)?;
        let kind = match item {
            responses::ResponseOutputItem::FunctionCall(ref call) => ModelOutputKind::ToolIntent {
                model_call_id: call.call_id.clone(),
            },
            responses::ResponseOutputItem::Reasoning(_) => ModelOutputKind::Reasoning,
            responses::ResponseOutputItem::Message(_) => return Ok(StreamMapping::empty()),
            responses::ResponseOutputItem::Extension(_) => {
                return Err(CompatibleProviderError::UnsupportedExtension.into())
            }
        };
        self.start(ResponsesKey::Item(output_index), kind)
    }

    fn responses_content_start(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<StreamMapping, StreamMappingError> {
        let key = ResponsesKey::Content(
            required_u64(object, "output_index")?,
            required_u64(object, "content_index")?,
        );
        let part: responses::OutputContent = serde_json::from_value(required(object, "part")?)
            .map_err(|_| StreamMappingError::ProtocolInvariant)?;
        let kind = match part {
            responses::OutputContent::OutputText(_) => ModelOutputKind::Text,
            responses::OutputContent::Refusal(_) => ModelOutputKind::Refusal,
            responses::OutputContent::Extension(_) => {
                return Err(CompatibleProviderError::UnsupportedExtension.into())
            }
        };
        self.start(key, kind)
    }

    fn start(
        &mut self,
        key: ResponsesKey,
        kind: ModelOutputKind,
    ) -> Result<StreamMapping, StreamMappingError> {
        let neutral_index = self.next_index;
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        if self
            .open
            .insert(
                key,
                ResponsesOpen {
                    neutral_index,
                    kind: kind.clone(),
                },
            )
            .is_some()
        {
            return Err(StreamMappingError::ProtocolInvariant);
        }
        Ok(StreamMapping::events(vec![
            ModelStreamEvent::OutputItemStarted {
                output_index: neutral_index,
                kind,
            },
        ]))
    }

    fn responses_delta(
        &self,
        object: &Map<String, Value>,
        field: &str,
        delta_kind: Delta,
    ) -> Result<StreamMapping, StreamMappingError> {
        let output = required_u64(object, "output_index")?;
        let key = if matches!(delta_kind, Delta::Text | Delta::Refusal) {
            ResponsesKey::Content(output, required_u64(object, "content_index")?)
        } else {
            ResponsesKey::Item(output)
        };
        let open = self
            .open
            .get(&key)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let delta = object
            .get(field)
            .and_then(Value::as_str)
            .ok_or(StreamMappingError::ProtocolInvariant)?
            .to_owned();
        let event = match delta_kind {
            Delta::Text => ModelStreamEvent::TextDelta {
                output_index: open.neutral_index,
                delta,
            },
            Delta::Refusal => ModelStreamEvent::RefusalDelta {
                output_index: open.neutral_index,
                delta,
            },
            Delta::Reasoning => ModelStreamEvent::ReasoningDelta {
                output_index: open.neutral_index,
                delta,
            },
            Delta::ToolArguments => {
                let ModelOutputKind::ToolIntent { model_call_id } = &open.kind else {
                    return Err(StreamMappingError::ProtocolInvariant);
                };
                ModelStreamEvent::ToolArgumentsDelta {
                    output_index: open.neutral_index,
                    model_call_id: model_call_id.clone(),
                    delta,
                }
            }
        };
        Ok(StreamMapping::events(vec![event]))
    }

    fn responses_content_done(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<StreamMapping, StreamMappingError> {
        let key = ResponsesKey::Content(
            required_u64(object, "output_index")?,
            required_u64(object, "content_index")?,
        );
        let open = self
            .open
            .remove(&key)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let part: responses::OutputContent = serde_json::from_value(required(object, "part")?)
            .map_err(|_| StreamMappingError::ProtocolInvariant)?;
        let item = match part {
            responses::OutputContent::OutputText(value) => ModelItem::Text { text: value.text },
            responses::OutputContent::Refusal(value) => ModelItem::Refusal {
                text: value.refusal,
            },
            responses::OutputContent::Extension(_) => {
                return Err(CompatibleProviderError::UnsupportedExtension.into())
            }
        };
        Ok(completed(open.neutral_index, item))
    }

    fn responses_item_done(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<StreamMapping, StreamMappingError> {
        let key = ResponsesKey::Item(required_u64(object, "output_index")?);
        let Some(open) = self.open.remove(&key) else {
            return Ok(StreamMapping::empty());
        };
        let item: responses::ResponseOutputItem = serde_json::from_value(required(object, "item")?)
            .map_err(|_| StreamMappingError::ProtocolInvariant)?;
        let mut normalized = crate::outcome::responses_items(&[item], self.reasoning_visibility)?;
        if normalized.is_empty() {
            return Err(StreamMappingError::ProtocolInvariant);
        }
        Ok(completed(open.neutral_index, normalized.remove(0)))
    }
}

#[derive(Clone, Copy)]
enum Delta {
    Text,
    Refusal,
    Reasoning,
    ToolArguments,
}

#[derive(Clone, Debug)]
enum MessagesBlock {
    Text(String),
    Tool {
        id: String,
        name: String,
        arguments: String,
    },
    Thinking {
        text: String,
        signature: String,
    },
    Redacted(String),
}

/// Stateful semantic mapper for adapter-validated Messages stream events.
#[derive(Clone, Debug)]
pub struct MessagesStreamMapper {
    reasoning_visibility: bool,
    open: BTreeMap<u32, MessagesBlock>,
    items: Vec<ModelItem>,
    input_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
    output_tokens: Option<u64>,
    stop_reason: Option<messages::StopReason>,
}

impl MessagesStreamMapper {
    /// Creates a mapper with the request's frozen reasoning visibility.
    pub fn new(reasoning_visibility: bool) -> Self {
        Self {
            reasoning_visibility,
            open: BTreeMap::new(),
            items: Vec::new(),
            input_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            output_tokens: None,
            stop_reason: None,
        }
    }

    /// Converts one event already validated by the protocol adapter.
    pub fn accept(
        &mut self,
        event: &messages::StreamEvent,
    ) -> Result<StreamMapping, StreamMappingError> {
        use messages::StreamEventKind as Kind;
        match event.kind() {
            Kind::MessageStart => {
                let message = event
                    .value()
                    .get("message")
                    .and_then(Value::as_object)
                    .ok_or(StreamMappingError::ProtocolInvariant)?;
                let usage = message
                    .get("usage")
                    .and_then(Value::as_object)
                    .ok_or(StreamMappingError::ProtocolInvariant)?;
                self.input_tokens = usage.get("input_tokens").and_then(Value::as_u64);
                self.cache_read_tokens =
                    usage.get("cache_read_input_tokens").and_then(Value::as_u64);
                self.cache_write_tokens = usage
                    .get("cache_creation_input_tokens")
                    .and_then(Value::as_u64);
                Ok(StreamMapping::empty())
            }
            Kind::ContentBlockStart => self.messages_start(event),
            Kind::ContentBlockDelta(kind) => self.messages_delta(event, kind),
            Kind::ContentBlockStop => self.messages_stop(event),
            Kind::MessageDelta => {
                let value = event.value();
                self.output_tokens = value
                    .get("usage")
                    .and_then(Value::as_object)
                    .and_then(|usage| usage.get("output_tokens"))
                    .and_then(Value::as_u64);
                self.stop_reason = value
                    .get("delta")
                    .and_then(Value::as_object)
                    .and_then(|delta| delta.get("stop_reason"))
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|_| StreamMappingError::ProtocolInvariant)?;
                Ok(StreamMapping::events(vec![
                    ModelStreamEvent::UsageUpdated {
                        usage: self.usage(),
                    },
                ]))
            }
            Kind::MessageStop => self.messages_terminal(),
            Kind::Ping => Ok(StreamMapping::empty()),
            Kind::Error => Err(StreamMappingError::Provider(
                CompatibleProviderError::UnclassifiedProtocolError,
            )),
            Kind::Extension(_) => Err(CompatibleProviderError::UnsupportedExtension.into()),
        }
    }

    fn messages_start(
        &mut self,
        event: &messages::StreamEvent,
    ) -> Result<StreamMapping, StreamMappingError> {
        let index = event_index(event)?;
        let block = event
            .value()
            .get("content_block")
            .and_then(Value::as_object)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let (state, output_kind) = match kind {
            "text" => (
                MessagesBlock::Text(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                ),
                ModelOutputKind::Text,
            ),
            "tool_use" => {
                let id = required_str(block, "id")?.to_owned();
                (
                    MessagesBlock::Tool {
                        id: id.clone(),
                        name: required_str(block, "name")?.to_owned(),
                        arguments: String::new(),
                    },
                    ModelOutputKind::ToolIntent { model_call_id: id },
                )
            }
            "thinking" => (
                MessagesBlock::Thinking {
                    text: block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    signature: block
                        .get("signature")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                },
                ModelOutputKind::Reasoning,
            ),
            "redacted_thinking" => (
                MessagesBlock::Redacted(required_str(block, "data")?.to_owned()),
                ModelOutputKind::Reasoning,
            ),
            _ => return Err(CompatibleProviderError::UnsupportedExtension.into()),
        };
        if self.open.insert(index, state).is_some() {
            return Err(StreamMappingError::ProtocolInvariant);
        }
        Ok(StreamMapping::events(vec![
            ModelStreamEvent::OutputItemStarted {
                output_index: index,
                kind: output_kind,
            },
        ]))
    }

    fn messages_delta(
        &mut self,
        event: &messages::StreamEvent,
        kind: &messages::DeltaKind,
    ) -> Result<StreamMapping, StreamMappingError> {
        let index = event_index(event)?;
        let delta = event
            .value()
            .get("delta")
            .and_then(Value::as_object)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let block = self
            .open
            .get_mut(&index)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let mapped = match (block, kind) {
            (MessagesBlock::Text(text), messages::DeltaKind::Text) => {
                let value = required_str(delta, "text")?.to_owned();
                text.push_str(&value);
                Some(ModelStreamEvent::TextDelta {
                    output_index: index,
                    delta: value,
                })
            }
            (MessagesBlock::Tool { id, arguments, .. }, messages::DeltaKind::InputJson) => {
                let value = required_str(delta, "partial_json")?.to_owned();
                arguments.push_str(&value);
                Some(ModelStreamEvent::ToolArgumentsDelta {
                    output_index: index,
                    model_call_id: id.clone(),
                    delta: value,
                })
            }
            (MessagesBlock::Thinking { text, .. }, messages::DeltaKind::Thinking) => {
                let value = required_str(delta, "thinking")?.to_owned();
                text.push_str(&value);
                self.reasoning_visibility
                    .then_some(ModelStreamEvent::ReasoningDelta {
                        output_index: index,
                        delta: value,
                    })
            }
            (MessagesBlock::Thinking { signature, .. }, messages::DeltaKind::Signature) => {
                signature.push_str(required_str(delta, "signature")?);
                None
            }
            (_, messages::DeltaKind::Citation) => None,
            (_, messages::DeltaKind::Extension(_)) => {
                return Err(CompatibleProviderError::UnsupportedExtension.into())
            }
            _ => return Err(StreamMappingError::ProtocolInvariant),
        };
        Ok(StreamMapping::events(mapped.into_iter().collect()))
    }

    fn messages_stop(
        &mut self,
        event: &messages::StreamEvent,
    ) -> Result<StreamMapping, StreamMappingError> {
        let index = event_index(event)?;
        let block = self
            .open
            .remove(&index)
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let item = match block {
            MessagesBlock::Text(text) => ModelItem::Text { text },
            MessagesBlock::Tool {
                id,
                name,
                arguments,
            } => ModelItem::ToolIntent {
                model_call_id: id,
                tool_name: name,
                arguments_json: canonical_arguments(&arguments)?,
            },
            MessagesBlock::Thinking { text, signature: _ } if self.reasoning_visibility => {
                ModelItem::Reasoning {
                    content: ReasoningContent::ModelVisible(text),
                }
            }
            MessagesBlock::Thinking { signature, .. } => ModelItem::Reasoning {
                content: ReasoningContent::OpaqueReference(signature),
            },
            MessagesBlock::Redacted(data) => ModelItem::Reasoning {
                content: ReasoningContent::OpaqueReference(data),
            },
        };
        self.items.push(item.clone());
        Ok(completed(index, item))
    }

    fn messages_terminal(&self) -> Result<StreamMapping, StreamMappingError> {
        let reason = self
            .stop_reason
            .ok_or(StreamMappingError::ProtocolInvariant)?;
        let mut items = self.items.clone();
        if reason == messages::StopReason::Refusal {
            for item in &mut items {
                if let ModelItem::Text { text } = item {
                    *item = ModelItem::Refusal { text: text.clone() };
                }
            }
        }
        let terminal = match reason {
            messages::StopReason::MaxTokens => InvokeOutcome::Interrupted {
                kind: InterruptionKind::OutputLimit,
                partial_items: items,
                usage: self.usage(),
            },
            messages::StopReason::ModelContextWindowExceeded => InvokeOutcome::Rejected {
                kind: garive_llm::RejectionKind::ContextOverflow,
                sanitized_evidence: "model_context_window_exceeded".into(),
            },
            other => InvokeOutcome::Completed {
                items,
                usage: self.usage(),
                stop_reason: messages_stop_reason(other),
            },
        };
        Ok(StreamMapping::terminal(terminal))
    }

    fn usage(&self) -> ModelUsage {
        ModelUsage {
            input_tokens: self
                .input_tokens
                .map_or(TokenCount::Unknown, TokenCount::Known),
            output_tokens: self
                .output_tokens
                .map_or(TokenCount::Unknown, TokenCount::Known),
            cache_read_tokens: self.cache_read_tokens.map(TokenCount::Known),
            cache_write_tokens: self.cache_write_tokens.map(TokenCount::Known),
            source: UsageSource::ProviderReported,
        }
    }
}

fn completed(output_index: u32, item: ModelItem) -> StreamMapping {
    StreamMapping::events(vec![ModelStreamEvent::OutputItemCompleted {
        output_index,
        item,
    }])
}

fn canonical_arguments(encoded: &str) -> Result<String, StreamMappingError> {
    let value: Value =
        serde_json::from_str(encoded).map_err(|_| StreamMappingError::ProtocolInvariant)?;
    let Value::Object(object) = value else {
        return Err(StreamMappingError::ProtocolInvariant);
    };
    serde_json::to_string(&object).map_err(|_| StreamMappingError::ProtocolInvariant)
}

fn messages_stop_reason(reason: messages::StopReason) -> ModelStopReason {
    match reason {
        messages::StopReason::EndTurn => ModelStopReason::EndTurn,
        messages::StopReason::StopSequence => ModelStopReason::StopSequence,
        messages::StopReason::ToolUse => ModelStopReason::ToolUse,
        messages::StopReason::PauseTurn => ModelStopReason::PauseTurn,
        messages::StopReason::Refusal => ModelStopReason::Refusal,
        messages::StopReason::MaxTokens | messages::StopReason::ModelContextWindowExceeded => {
            unreachable!("handled by non-completed outcomes")
        }
    }
}

fn event_index(event: &messages::StreamEvent) -> Result<u32, StreamMappingError> {
    event
        .index()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(StreamMappingError::ProtocolInvariant)
}

fn required(object: &Map<String, Value>, field: &str) -> Result<Value, StreamMappingError> {
    object
        .get(field)
        .cloned()
        .ok_or(StreamMappingError::ProtocolInvariant)
}

fn required_u64(object: &Map<String, Value>, field: &str) -> Result<u64, StreamMappingError> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or(StreamMappingError::ProtocolInvariant)
}

fn required_str<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, StreamMappingError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or(StreamMappingError::ProtocolInvariant)
}
