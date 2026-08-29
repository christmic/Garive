//! Typed incremental Messages events and lifecycle validation.

use crate::{wire, ErrorEnvelope, MessageResponse, MessagesAdapterError, SseDecoder, SseFrame};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Portable Messages stream event kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamEventKind {
    /// Initial message snapshot.
    MessageStart,
    /// Initial content-block snapshot.
    ContentBlockStart,
    /// Incremental content-block delta.
    ContentBlockDelta(DeltaKind),
    /// Content block terminal.
    ContentBlockStop,
    /// Message terminal fields and usage.
    MessageDelta,
    /// Successful message terminal.
    MessageStop,
    /// Liveness event.
    Ping,
    /// Protocol error terminal.
    Error,
    /// Future event retained losslessly.
    Extension(String),
}

/// Portable content-block delta kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeltaKind {
    /// Text fragment.
    Text,
    /// Partial tool-input JSON.
    InputJson,
    /// Thinking fragment.
    Thinking,
    /// Thinking integrity signature.
    Signature,
    /// Citation object.
    Citation,
    /// Future delta retained losslessly.
    Extension(String),
}

/// One typed event with its complete original JSON object.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamEvent {
    kind: StreamEventKind,
    value: Map<String, Value>,
}

impl StreamEvent {
    /// Returns the typed portable event kind.
    pub fn kind(&self) -> &StreamEventKind {
        &self.kind
    }
    /// Returns the complete lossless event object.
    pub fn value(&self) -> &Map<String, Value> {
        &self.value
    }
    /// Returns a content-block index when present.
    pub fn index(&self) -> Option<u64> {
        self.value.get("index").and_then(Value::as_u64)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpenBlock {
    kind: String,
    partial_json: String,
}

/// Incremental SSE and Messages lifecycle decoder for one exchange.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MessagesStreamDecoder {
    sse: SseDecoder,
    started: bool,
    terminal: bool,
    message_delta: bool,
    blocks: BTreeMap<u64, OpenBlock>,
}

impl MessagesStreamDecoder {
    /// Creates a decoder for one streaming response.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends arbitrary transport bytes and emits every completed event immediately.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamEvent>, MessagesAdapterError> {
        let frames = self.sse.push(bytes)?;
        frames
            .into_iter()
            .map(|frame| self.accept_frame(frame))
            .collect()
    }

    /// Requires a successful or protocol-error terminal and no open blocks at EOF.
    pub fn finish(&mut self) -> Result<(), MessagesAdapterError> {
        self.sse.finish()?;
        if self.terminal && self.blocks.is_empty() {
            Ok(())
        } else {
            Err(MessagesAdapterError::TruncatedStream)
        }
    }

    fn accept_frame(&mut self, frame: SseFrame) -> Result<StreamEvent, MessagesAdapterError> {
        let value: Value =
            serde_json::from_str(frame.data()).map_err(|_| MessagesAdapterError::InvalidJson)?;
        let object = value
            .as_object()
            .ok_or(MessagesAdapterError::InvalidJson)?
            .clone();
        let discriminator = object
            .get(wire::FIELD_TYPE)
            .and_then(Value::as_str)
            .ok_or(MessagesAdapterError::InvalidJson)?;
        if frame.event().is_some_and(|event| event != discriminator) {
            return Err(MessagesAdapterError::InvalidSse);
        }
        let kind = event_kind(discriminator, &object)?;
        self.validate_lifecycle(&kind, &object)?;
        Ok(StreamEvent {
            kind,
            value: object,
        })
    }

    fn validate_lifecycle(
        &mut self,
        kind: &StreamEventKind,
        value: &Map<String, Value>,
    ) -> Result<(), MessagesAdapterError> {
        if self.terminal {
            return Err(MessagesAdapterError::InvalidLifecycle(
                "Messages event arrived after terminal",
            ));
        }
        match kind {
            StreamEventKind::Ping => return Ok(()),
            StreamEventKind::MessageStart if !self.started => {
                let message: MessageResponse = serde_json::from_value(
                    value
                        .get("message")
                        .cloned()
                        .ok_or(MessagesAdapterError::InvalidJson)?,
                )
                .map_err(|_| MessagesAdapterError::InvalidJson)?;
                message.validate()?;
                self.started = true;
            }
            StreamEventKind::MessageStart => {
                return Err(MessagesAdapterError::InvalidLifecycle(
                    "Messages stream has duplicate message_start",
                ))
            }
            StreamEventKind::Error => {
                let error: ErrorEnvelope = serde_json::from_value(Value::Object(value.clone()))
                    .map_err(|_| MessagesAdapterError::InvalidJson)?;
                if error.r#type != wire::KIND_ERROR
                    || error.error.r#type.is_empty()
                    || error.error.message.is_empty()
                {
                    return Err(MessagesAdapterError::InvalidJson);
                }
                self.terminal = true;
            }
            _ if !self.started => {
                return Err(MessagesAdapterError::InvalidLifecycle(
                    "Messages event precedes message_start",
                ))
            }
            StreamEventKind::ContentBlockStart => self.start_block(value)?,
            StreamEventKind::ContentBlockDelta(delta) => self.delta_block(value, delta)?,
            StreamEventKind::ContentBlockStop => self.stop_block(value)?,
            StreamEventKind::MessageDelta if !self.message_delta && self.blocks.is_empty() => {
                if value.get("delta").and_then(Value::as_object).is_none()
                    || value
                        .get("usage")
                        .and_then(Value::as_object)
                        .and_then(|usage| usage.get("output_tokens"))
                        .and_then(Value::as_u64)
                        .is_none()
                {
                    return Err(MessagesAdapterError::InvalidJson);
                }
                self.message_delta = true
            }
            StreamEventKind::MessageDelta => {
                return Err(MessagesAdapterError::InvalidLifecycle(
                    "Messages message_delta is duplicate or precedes block stop",
                ))
            }
            StreamEventKind::MessageStop if self.message_delta && self.blocks.is_empty() => {
                self.terminal = true
            }
            StreamEventKind::MessageStop => {
                return Err(MessagesAdapterError::InvalidLifecycle(
                    "Messages message_stop precedes terminal delta or block stop",
                ))
            }
            StreamEventKind::Extension(_) => {}
        }
        Ok(())
    }

    fn start_block(&mut self, value: &Map<String, Value>) -> Result<(), MessagesAdapterError> {
        let index = required_index(value)?;
        let kind = value
            .get(wire::FIELD_CONTENT_BLOCK)
            .and_then(Value::as_object)
            .and_then(|block| block.get(wire::FIELD_TYPE))
            .and_then(Value::as_str)
            .ok_or(MessagesAdapterError::InvalidJson)?
            .to_owned();
        let block = value
            .get("content_block")
            .and_then(Value::as_object)
            .ok_or(MessagesAdapterError::InvalidJson)?;
        validate_start_block(&kind, block)?;
        if self
            .blocks
            .insert(
                index,
                OpenBlock {
                    kind,
                    partial_json: String::new(),
                },
            )
            .is_some()
        {
            return Err(MessagesAdapterError::InvalidLifecycle(
                "Messages content block index is already open",
            ));
        }
        Ok(())
    }

    fn delta_block(
        &mut self,
        value: &Map<String, Value>,
        delta: &DeltaKind,
    ) -> Result<(), MessagesAdapterError> {
        let index = required_index(value)?;
        let block = self
            .blocks
            .get_mut(&index)
            .ok_or(MessagesAdapterError::InvalidLifecycle(
                "Messages delta targets no open block",
            ))?;
        let compatible = matches!(
            (block.kind.as_str(), delta),
            (wire::KIND_TEXT, DeltaKind::Text | DeltaKind::Citation)
                | (wire::KIND_TOOL_USE, DeltaKind::InputJson)
                | (
                    wire::KIND_THINKING,
                    DeltaKind::Thinking | DeltaKind::Signature
                )
        );
        if !compatible && !matches!(delta, DeltaKind::Extension(_)) {
            return Err(MessagesAdapterError::InvalidLifecycle(
                "Messages delta kind does not match its block",
            ));
        }
        if matches!(delta, DeltaKind::InputJson) {
            let fragment = value
                .get(wire::FIELD_DELTA)
                .and_then(Value::as_object)
                .and_then(|delta| delta.get(wire::FIELD_PARTIAL_JSON))
                .and_then(Value::as_str)
                .ok_or(MessagesAdapterError::InvalidJson)?;
            block.partial_json.push_str(fragment);
        }
        Ok(())
    }

    fn stop_block(&mut self, value: &Map<String, Value>) -> Result<(), MessagesAdapterError> {
        let index = required_index(value)?;
        let block = self
            .blocks
            .remove(&index)
            .ok_or(MessagesAdapterError::InvalidLifecycle(
                "Messages stop targets no open block",
            ))?;
        if block.kind == wire::KIND_TOOL_USE
            && serde_json::from_str::<Value>(&block.partial_json).is_err()
        {
            return Err(MessagesAdapterError::InvalidJson);
        }
        Ok(())
    }
}

fn event_kind(
    discriminator: &str,
    value: &Map<String, Value>,
) -> Result<StreamEventKind, MessagesAdapterError> {
    Ok(match discriminator {
        wire::EVENT_MESSAGE_START => StreamEventKind::MessageStart,
        wire::EVENT_CONTENT_BLOCK_START => StreamEventKind::ContentBlockStart,
        wire::EVENT_CONTENT_BLOCK_DELTA => StreamEventKind::ContentBlockDelta(delta_kind(value)?),
        wire::EVENT_CONTENT_BLOCK_STOP => StreamEventKind::ContentBlockStop,
        wire::EVENT_MESSAGE_DELTA => StreamEventKind::MessageDelta,
        wire::EVENT_MESSAGE_STOP => StreamEventKind::MessageStop,
        wire::EVENT_PING => StreamEventKind::Ping,
        wire::KIND_ERROR => StreamEventKind::Error,
        other => StreamEventKind::Extension(other.to_owned()),
    })
}

fn delta_kind(value: &Map<String, Value>) -> Result<DeltaKind, MessagesAdapterError> {
    let delta = value
        .get(wire::FIELD_DELTA)
        .and_then(Value::as_object)
        .ok_or(MessagesAdapterError::InvalidJson)?;
    let kind = delta
        .get(wire::FIELD_TYPE)
        .and_then(Value::as_str)
        .ok_or(MessagesAdapterError::InvalidJson)?;
    Ok(match kind {
        wire::DELTA_TEXT
            if delta
                .get(wire::FIELD_TEXT)
                .and_then(Value::as_str)
                .is_some() =>
        {
            DeltaKind::Text
        }
        wire::DELTA_INPUT_JSON
            if delta
                .get(wire::FIELD_PARTIAL_JSON)
                .and_then(Value::as_str)
                .is_some() =>
        {
            DeltaKind::InputJson
        }
        wire::DELTA_THINKING
            if delta
                .get(wire::FIELD_THINKING)
                .and_then(Value::as_str)
                .is_some() =>
        {
            DeltaKind::Thinking
        }
        wire::DELTA_SIGNATURE
            if delta
                .get(wire::FIELD_SIGNATURE)
                .and_then(Value::as_str)
                .is_some() =>
        {
            DeltaKind::Signature
        }
        wire::DELTA_CITATIONS if delta.contains_key(wire::FIELD_CITATION) => DeltaKind::Citation,
        wire::DELTA_TEXT
        | wire::DELTA_INPUT_JSON
        | wire::DELTA_THINKING
        | wire::DELTA_SIGNATURE
        | wire::DELTA_CITATIONS => return Err(MessagesAdapterError::InvalidJson),
        other => DeltaKind::Extension(other.to_owned()),
    })
}

fn validate_start_block(
    kind: &str,
    block: &Map<String, Value>,
) -> Result<(), MessagesAdapterError> {
    let valid = match kind {
        wire::KIND_TEXT => block
            .get(wire::FIELD_TEXT)
            .and_then(Value::as_str)
            .is_some(),
        wire::KIND_THINKING => {
            block
                .get(wire::FIELD_THINKING)
                .and_then(Value::as_str)
                .is_some()
                && block
                    .get(wire::FIELD_SIGNATURE)
                    .and_then(Value::as_str)
                    .is_some()
        }
        wire::KIND_REDACTED_THINKING => block
            .get("data")
            .and_then(Value::as_str)
            .is_some_and(|data| !data.is_empty()),
        wire::KIND_TOOL_USE => {
            block
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
                && block
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.is_empty())
                && block.get("input").and_then(Value::as_object).is_some()
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(MessagesAdapterError::InvalidJson)
    }
}

fn required_index(value: &Map<String, Value>) -> Result<u64, MessagesAdapterError> {
    value
        .get("index")
        .and_then(Value::as_u64)
        .ok_or(MessagesAdapterError::InvalidJson)
}
