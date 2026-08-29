//! Portable Responses stream discriminators with lossless event payloads.

use crate::{
    OutputContent, ProtocolExtension, ReasoningPart, Response, ResponseOutputItem,
    ResponsesAdapterError,
};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// Core stream event discriminators from the pinned official SDK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableEventKind {
    /// Response object created.
    Created,
    /// Response queued.
    Queued,
    /// Response execution started.
    InProgress,
    /// Response completed.
    Completed,
    /// Response failed.
    Failed,
    /// Response became incomplete.
    Incomplete,
    /// Protocol error event.
    Error,
    /// Output item added.
    OutputItemAdded,
    /// Output item completed.
    OutputItemDone,
    /// Message content part added.
    ContentPartAdded,
    /// Message content part completed.
    ContentPartDone,
    /// Output text delta.
    OutputTextDelta,
    /// Output text completed.
    OutputTextDone,
    /// Refusal delta.
    RefusalDelta,
    /// Refusal completed.
    RefusalDone,
    /// Function arguments delta.
    FunctionArgumentsDelta,
    /// Function arguments completed.
    FunctionArgumentsDone,
    /// Reasoning summary part added.
    ReasoningSummaryPartAdded,
    /// Reasoning summary part completed.
    ReasoningSummaryPartDone,
    /// Reasoning summary text delta.
    ReasoningSummaryTextDelta,
    /// Reasoning summary text completed.
    ReasoningSummaryTextDone,
    /// Reasoning text delta.
    ReasoningTextDelta,
    /// Reasoning text completed.
    ReasoningTextDone,
    /// Output text annotation added.
    OutputTextAnnotationAdded,
}

impl PortableEventKind {
    /// Returns the exact wire discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "response.created",
            Self::Queued => "response.queued",
            Self::InProgress => "response.in_progress",
            Self::Completed => "response.completed",
            Self::Failed => "response.failed",
            Self::Incomplete => "response.incomplete",
            Self::Error => "error",
            Self::OutputItemAdded => "response.output_item.added",
            Self::OutputItemDone => "response.output_item.done",
            Self::ContentPartAdded => "response.content_part.added",
            Self::ContentPartDone => "response.content_part.done",
            Self::OutputTextDelta => "response.output_text.delta",
            Self::OutputTextDone => "response.output_text.done",
            Self::RefusalDelta => "response.refusal.delta",
            Self::RefusalDone => "response.refusal.done",
            Self::FunctionArgumentsDelta => "response.function_call_arguments.delta",
            Self::FunctionArgumentsDone => "response.function_call_arguments.done",
            Self::ReasoningSummaryPartAdded => "response.reasoning_summary_part.added",
            Self::ReasoningSummaryPartDone => "response.reasoning_summary_part.done",
            Self::ReasoningSummaryTextDelta => "response.reasoning_summary_text.delta",
            Self::ReasoningSummaryTextDone => "response.reasoning_summary_text.done",
            Self::ReasoningTextDelta => "response.reasoning_text.delta",
            Self::ReasoningTextDone => "response.reasoning_text.done",
            Self::OutputTextAnnotationAdded => "response.output_text.annotation.added",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        ALL_KINDS
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }
}

const ALL_KINDS: &[PortableEventKind] = &[
    PortableEventKind::Created,
    PortableEventKind::Queued,
    PortableEventKind::InProgress,
    PortableEventKind::Completed,
    PortableEventKind::Failed,
    PortableEventKind::Incomplete,
    PortableEventKind::Error,
    PortableEventKind::OutputItemAdded,
    PortableEventKind::OutputItemDone,
    PortableEventKind::ContentPartAdded,
    PortableEventKind::ContentPartDone,
    PortableEventKind::OutputTextDelta,
    PortableEventKind::OutputTextDone,
    PortableEventKind::RefusalDelta,
    PortableEventKind::RefusalDone,
    PortableEventKind::FunctionArgumentsDelta,
    PortableEventKind::FunctionArgumentsDone,
    PortableEventKind::ReasoningSummaryPartAdded,
    PortableEventKind::ReasoningSummaryPartDone,
    PortableEventKind::ReasoningSummaryTextDelta,
    PortableEventKind::ReasoningSummaryTextDone,
    PortableEventKind::ReasoningTextDelta,
    PortableEventKind::ReasoningTextDone,
    PortableEventKind::OutputTextAnnotationAdded,
];

/// Typed portable event or lossless hosted/future extension event.
#[derive(Clone, Debug, PartialEq)]
pub enum ResponseStreamEvent {
    /// A validated event in the portable catalogue.
    Portable {
        /// Typed discriminator.
        kind: PortableEventKind,
        /// Strictly non-negative protocol sequence number.
        sequence_number: u64,
        /// Lossless original event object.
        object: Map<String, Value>,
    },
    /// Hosted, future, or Provider-specific event.
    Extension(ProtocolExtension),
}

impl ResponseStreamEvent {
    /// Returns the event wire discriminator.
    pub fn discriminator(&self) -> &str {
        match self {
            Self::Portable { kind, .. } => kind.as_str(),
            Self::Extension(extension) => extension.discriminator(),
        }
    }

    /// Returns the sequence number when supplied by the protocol.
    pub fn sequence_number(&self) -> Option<u64> {
        match self {
            Self::Portable {
                sequence_number, ..
            } => Some(*sequence_number),
            Self::Extension(extension) => extension
                .object()
                .get("sequence_number")
                .and_then(Value::as_u64),
        }
    }

    /// Returns the lossless original JSON object.
    pub fn object(&self) -> &Map<String, Value> {
        match self {
            Self::Portable { object, .. } => object,
            Self::Extension(extension) => extension.object(),
        }
    }
}

impl Serialize for ResponseStreamEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.object().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ResponseStreamEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("Responses event must be an object"))?;
        let discriminator = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("Responses event requires type"))?;
        let Some(kind) = PortableEventKind::parse(discriminator) else {
            return ProtocolExtension::new(discriminator, object.clone())
                .map(Self::Extension)
                .map_err(D::Error::custom);
        };
        let sequence_number = required_u64(object, "sequence_number").map_err(D::Error::custom)?;
        validate_payload(kind, object).map_err(D::Error::custom)?;
        Ok(Self::Portable {
            kind,
            sequence_number,
            object: object.clone(),
        })
    }
}

fn validate_payload(
    kind: PortableEventKind,
    object: &Map<String, Value>,
) -> Result<(), ResponsesAdapterError> {
    match kind {
        PortableEventKind::Created
        | PortableEventKind::Queued
        | PortableEventKind::InProgress
        | PortableEventKind::Completed
        | PortableEventKind::Failed
        | PortableEventKind::Incomplete => {
            let response: Response = serde_json::from_value(required(object, "response")?.clone())
                .map_err(|_| ResponsesAdapterError::InvalidJson)?;
            response.validate()?;
        }
        PortableEventKind::OutputItemAdded | PortableEventKind::OutputItemDone => {
            required_u64(object, "output_index")?;
            serde_json::from_value::<ResponseOutputItem>(required(object, "item")?.clone())
                .map_err(|_| ResponsesAdapterError::InvalidJson)?;
        }
        PortableEventKind::ContentPartAdded | PortableEventKind::ContentPartDone => {
            required_indexes(object, &["output_index", "content_index"])?;
            required_text(object, "item_id")?;
            serde_json::from_value::<OutputContent>(required(object, "part")?.clone())
                .map_err(|_| ResponsesAdapterError::InvalidJson)?;
        }
        PortableEventKind::OutputTextDelta
        | PortableEventKind::OutputTextDone
        | PortableEventKind::RefusalDelta
        | PortableEventKind::RefusalDone => {
            required_indexes(object, &["output_index", "content_index"])?;
            required_text(object, "item_id")?;
            required_text(
                object,
                if matches!(
                    kind,
                    PortableEventKind::OutputTextDelta | PortableEventKind::RefusalDelta
                ) {
                    "delta"
                } else if kind == PortableEventKind::RefusalDone {
                    "refusal"
                } else {
                    "text"
                },
            )?;
        }
        PortableEventKind::FunctionArgumentsDelta | PortableEventKind::FunctionArgumentsDone => {
            required_u64(object, "output_index")?;
            required_text(object, "item_id")?;
            required_text(
                object,
                if kind == PortableEventKind::FunctionArgumentsDelta {
                    "delta"
                } else {
                    "arguments"
                },
            )?;
        }
        PortableEventKind::ReasoningSummaryPartAdded
        | PortableEventKind::ReasoningSummaryPartDone => {
            required_indexes(object, &["output_index", "summary_index"])?;
            required_text(object, "item_id")?;
            serde_json::from_value::<ReasoningPart>(required(object, "part")?.clone())
                .map_err(|_| ResponsesAdapterError::InvalidJson)?;
        }
        PortableEventKind::ReasoningSummaryTextDelta
        | PortableEventKind::ReasoningSummaryTextDone => {
            required_indexes(object, &["output_index", "summary_index"])?;
            required_text(object, "item_id")?;
            required_text(
                object,
                if kind == PortableEventKind::ReasoningSummaryTextDelta {
                    "delta"
                } else {
                    "text"
                },
            )?;
        }
        PortableEventKind::ReasoningTextDelta | PortableEventKind::ReasoningTextDone => {
            required_indexes(object, &["output_index", "content_index"])?;
            required_text(object, "item_id")?;
            required_text(
                object,
                if kind == PortableEventKind::ReasoningTextDelta {
                    "delta"
                } else {
                    "text"
                },
            )?;
        }
        PortableEventKind::OutputTextAnnotationAdded => {
            required_indexes(
                object,
                &["output_index", "content_index", "annotation_index"],
            )?;
            required_text(object, "item_id")?;
            required(object, "annotation")?;
        }
        PortableEventKind::Error => {
            required_text(object, "message")?;
        }
    }
    Ok(())
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, ResponsesAdapterError> {
    object.get(key).ok_or(ResponsesAdapterError::InvalidJson)
}

fn required_text(object: &Map<String, Value>, key: &str) -> Result<(), ResponsesAdapterError> {
    match required(object, key)?.as_str() {
        Some(value) if !value.is_empty() => Ok(()),
        _ => Err(ResponsesAdapterError::InvalidJson),
    }
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, ResponsesAdapterError> {
    required(object, key)?
        .as_u64()
        .ok_or(ResponsesAdapterError::InvalidJson)
}

fn required_indexes(
    object: &Map<String, Value>,
    keys: &[&str],
) -> Result<(), ResponsesAdapterError> {
    keys.iter()
        .try_for_each(|key| required_u64(object, key).map(|_| ()))
}
