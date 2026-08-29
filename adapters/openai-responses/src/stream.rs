//! Responses event lifecycle validation layered over incremental SSE framing.

use crate::{
    wire, PortableEventKind, ResponseStreamEvent, ResponsesAdapter, ResponsesAdapterError,
    SseDecoder,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Incremental typed Responses stream decoder.
#[derive(Clone, Debug, Default)]
pub struct ResponsesStreamDecoder {
    sse: SseDecoder,
    last_sequence: Option<u64>,
    created: bool,
    terminal: bool,
    sentinel: bool,
    items: BTreeMap<u64, ItemState>,
}

#[derive(Clone, Debug)]
struct ItemState {
    id: String,
    kind: String,
    done: bool,
    content: BTreeSet<u64>,
    content_done: BTreeSet<u64>,
}

impl ResponsesStreamDecoder {
    /// Creates an empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends transport bytes and emits every completed typed event.
    pub fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<Vec<ResponseStreamEvent>, ResponsesAdapterError> {
        let mut events = Vec::new();
        for frame in self.sse.push(bytes)? {
            if frame.data() == "[DONE]" {
                if !self.terminal || self.sentinel || frame.event().is_some() {
                    return Err(ResponsesAdapterError::InvalidLifecycle(
                        "Responses DONE sentinel requires one preceding terminal",
                    ));
                }
                self.sentinel = true;
                continue;
            }
            if self.sentinel {
                return Err(ResponsesAdapterError::InvalidLifecycle(
                    "Responses event followed the DONE sentinel",
                ));
            }
            let event: ResponseStreamEvent = serde_json::from_str(frame.data())
                .map_err(|_| ResponsesAdapterError::InvalidJson)?;
            if frame
                .event()
                .is_some_and(|name| name != event.discriminator())
            {
                return Err(ResponsesAdapterError::InvalidSse);
            }
            self.accept(&event)?;
            events.push(event);
        }
        Ok(events)
    }

    /// Validates SSE framing and one complete Responses lifecycle at EOF.
    pub fn finish(&mut self) -> Result<(), ResponsesAdapterError> {
        self.sse.finish()?;
        if !self.terminal {
            return Err(ResponsesAdapterError::TruncatedStream);
        }
        Ok(())
    }

    fn accept(&mut self, event: &ResponseStreamEvent) -> Result<(), ResponsesAdapterError> {
        if self.terminal {
            return Err(ResponsesAdapterError::InvalidLifecycle(
                "Responses event followed its protocol terminal",
            ));
        }
        if let Some(sequence) = event.sequence_number() {
            if self
                .last_sequence
                .is_some_and(|previous| sequence <= previous)
            {
                return Err(ResponsesAdapterError::InvalidLifecycle(
                    "Responses sequence_number must increase",
                ));
            }
            self.last_sequence = Some(sequence);
        }
        let ResponseStreamEvent::Portable { kind, object, .. } = event else {
            if !self.created {
                return Err(ResponsesAdapterError::InvalidLifecycle(
                    "Responses extension event preceded response.created",
                ));
            }
            return Ok(());
        };
        match kind {
            PortableEventKind::Created => {
                if self.created || self.last_sequence != event.sequence_number() {
                    return Err(ResponsesAdapterError::InvalidLifecycle(
                        "Responses stream requires one initial response.created",
                    ));
                }
                self.created = true;
            }
            _ if !self.created => {
                return Err(ResponsesAdapterError::InvalidLifecycle(
                    "Responses event preceded response.created",
                ));
            }
            PortableEventKind::Completed => {
                if self.items.values().any(|item| !item.done) {
                    return Err(ResponsesAdapterError::InvalidLifecycle(
                        "Responses terminal has an open output item",
                    ));
                }
                self.terminal = true;
            }
            PortableEventKind::Failed
            | PortableEventKind::Incomplete
            | PortableEventKind::Error => self.terminal = true,
            PortableEventKind::OutputItemAdded => self.add_item(object)?,
            PortableEventKind::OutputItemDone => self.finish_item(object)?,
            PortableEventKind::ContentPartAdded => self.add_content(object)?,
            PortableEventKind::ContentPartDone => self.finish_content(object)?,
            PortableEventKind::OutputTextDelta
            | PortableEventKind::RefusalDelta
            | PortableEventKind::OutputTextAnnotationAdded => {
                self.require_content(object)?;
            }
            PortableEventKind::OutputTextDone | PortableEventKind::RefusalDone => {
                self.require_content(object)?;
            }
            PortableEventKind::FunctionArgumentsDelta
            | PortableEventKind::FunctionArgumentsDone => {
                self.require_item_kind(object, wire::KIND_FUNCTION_CALL)?;
            }
            PortableEventKind::ReasoningSummaryPartAdded
            | PortableEventKind::ReasoningSummaryPartDone
            | PortableEventKind::ReasoningSummaryTextDelta
            | PortableEventKind::ReasoningSummaryTextDone
            | PortableEventKind::ReasoningTextDelta
            | PortableEventKind::ReasoningTextDone => {
                self.require_item_kind(object, wire::KIND_REASONING)?;
            }
            PortableEventKind::Queued | PortableEventKind::InProgress => {}
        }
        Ok(())
    }

    fn add_item(&mut self, object: &Map<String, Value>) -> Result<(), ResponsesAdapterError> {
        let index = u64_field(object, "output_index")?;
        let item = object_field(object, "item")?;
        let id = text_field(item, "id")?;
        let kind = text_field(item, wire::FIELD_TYPE)?;
        if self.items.contains_key(&index) || self.items.values().any(|state| state.id == id) {
            return Err(lifecycle("Responses output item identity was reused"));
        }
        self.items.insert(
            index,
            ItemState {
                id: id.into(),
                kind: kind.into(),
                done: false,
                content: BTreeSet::new(),
                content_done: BTreeSet::new(),
            },
        );
        Ok(())
    }

    fn finish_item(&mut self, object: &Map<String, Value>) -> Result<(), ResponsesAdapterError> {
        let index = u64_field(object, "output_index")?;
        let item = object_field(object, "item")?;
        let id = text_field(item, "id")?;
        let kind = text_field(item, wire::FIELD_TYPE)?;
        let state = self
            .items
            .get_mut(&index)
            .ok_or_else(|| lifecycle("Responses output item done preceded added"))?;
        if state.done || state.id != id || state.kind != kind || state.content != state.content_done
        {
            return Err(lifecycle(
                "Responses output item done mismatched its open item",
            ));
        }
        state.done = true;
        Ok(())
    }

    fn add_content(&mut self, object: &Map<String, Value>) -> Result<(), ResponsesAdapterError> {
        let state = self.item_for(object)?;
        let content_index = u64_field(object, "content_index")?;
        if state.kind != wire::KIND_MESSAGE || state.done || !state.content.insert(content_index) {
            return Err(lifecycle("Responses content part identity was reused"));
        }
        Ok(())
    }

    fn finish_content(&mut self, object: &Map<String, Value>) -> Result<(), ResponsesAdapterError> {
        let content_index = u64_field(object, "content_index")?;
        let state = self.item_for(object)?;
        if !state.content.contains(&content_index) || !state.content_done.insert(content_index) {
            return Err(lifecycle("Responses content part done preceded added"));
        }
        Ok(())
    }

    fn require_content(&self, object: &Map<String, Value>) -> Result<(), ResponsesAdapterError> {
        let state = self.item_for_read(object)?;
        let content_index = u64_field(object, "content_index")?;
        if state.done
            || !state.content.contains(&content_index)
            || state.content_done.contains(&content_index)
        {
            return Err(lifecycle(
                "Responses content event targeted a closed or missing part",
            ));
        }
        Ok(())
    }

    fn require_item_kind(
        &self,
        object: &Map<String, Value>,
        kind: &str,
    ) -> Result<(), ResponsesAdapterError> {
        let state = self.item_for_read(object)?;
        if state.done || state.kind != kind {
            return Err(lifecycle(
                "Responses event targeted an incompatible output item",
            ));
        }
        Ok(())
    }

    fn item_for(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<&mut ItemState, ResponsesAdapterError> {
        let index = u64_field(object, "output_index")?;
        let id = text_field(object, "item_id")?;
        let state = self
            .items
            .get_mut(&index)
            .ok_or_else(|| lifecycle("Responses event targeted a missing output item"))?;
        if state.id != id {
            return Err(lifecycle("Responses event item_id mismatched output_index"));
        }
        Ok(state)
    }

    fn item_for_read(
        &self,
        object: &Map<String, Value>,
    ) -> Result<&ItemState, ResponsesAdapterError> {
        let index = u64_field(object, "output_index")?;
        let id = text_field(object, "item_id")?;
        let state = self
            .items
            .get(&index)
            .ok_or_else(|| lifecycle("Responses event targeted a missing output item"))?;
        if state.id != id {
            return Err(lifecycle("Responses event item_id mismatched output_index"));
        }
        Ok(state)
    }
}

impl ResponsesAdapter {
    /// Creates an incremental decoder for one streaming exchange.
    pub fn stream_decoder(&self) -> ResponsesStreamDecoder {
        ResponsesStreamDecoder::new()
    }
}

fn object_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, ResponsesAdapterError> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or(ResponsesAdapterError::InvalidJson)
}
fn text_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, ResponsesAdapterError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(ResponsesAdapterError::InvalidJson)
}
fn u64_field(object: &Map<String, Value>, key: &str) -> Result<u64, ResponsesAdapterError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(ResponsesAdapterError::InvalidJson)
}
fn lifecycle(reason: &'static str) -> ResponsesAdapterError {
    ResponsesAdapterError::InvalidLifecycle(reason)
}
