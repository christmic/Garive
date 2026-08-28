use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};

use crate::{InvokeOutcome, ModelItem, ModelRequest, ModelUsage};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelOutputKind {
    Text,
    Refusal,
    Reasoning,
    ToolIntent { model_call_id: String },
    ToolObservation,
    MediaReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelStreamEvent {
    OutputItemStarted {
        output_index: u32,
        kind: ModelOutputKind,
    },
    TextDelta {
        output_index: u32,
        delta: String,
    },
    RefusalDelta {
        output_index: u32,
        delta: String,
    },
    ReasoningDelta {
        output_index: u32,
        delta: String,
    },
    ToolArgumentsDelta {
        output_index: u32,
        model_call_id: String,
        delta: String,
    },
    OutputItemCompleted {
        output_index: u32,
        item: ModelItem,
    },
    UsageUpdated {
        usage: ModelUsage,
    },
}

#[derive(Default)]
pub struct StreamValidator {
    started: BTreeMap<u32, ModelOutputKind>,
    completed: BTreeSet<u32>,
    last_started: Option<u32>,
}

impl StreamValidator {
    pub fn accept(&mut self, event: &ModelStreamEvent) -> Result<(), StreamInvariantError> {
        match event {
            ModelStreamEvent::OutputItemStarted { output_index, kind } => {
                if self.last_started.is_some_and(|last| *output_index <= last) {
                    return Err(StreamInvariantError::NonMonotonicStart);
                }
                self.started.insert(*output_index, kind.clone());
                self.last_started = Some(*output_index);
            }
            ModelStreamEvent::TextDelta { output_index, .. } => {
                self.require_kind(*output_index, &ModelOutputKind::Text)?;
            }
            ModelStreamEvent::RefusalDelta { output_index, .. } => {
                self.require_kind(*output_index, &ModelOutputKind::Refusal)?;
            }
            ModelStreamEvent::ReasoningDelta { output_index, .. } => {
                self.require_kind(*output_index, &ModelOutputKind::Reasoning)?;
            }
            ModelStreamEvent::ToolArgumentsDelta {
                output_index,
                model_call_id,
                ..
            } => self.require_kind(
                *output_index,
                &ModelOutputKind::ToolIntent {
                    model_call_id: model_call_id.clone(),
                },
            )?,
            ModelStreamEvent::OutputItemCompleted { output_index, item } => {
                if self.completed.contains(output_index) {
                    return Err(StreamInvariantError::ItemAlreadyCompleted);
                }
                self.require_kind(*output_index, &kind_of(item))?;
                self.completed.insert(*output_index);
            }
            ModelStreamEvent::UsageUpdated { .. } => {}
        }
        Ok(())
    }

    fn require_kind(
        &self,
        output_index: u32,
        expected: &ModelOutputKind,
    ) -> Result<(), StreamInvariantError> {
        let Some(actual) = self.started.get(&output_index) else {
            return Err(StreamInvariantError::ItemNotStarted);
        };
        if self.completed.contains(&output_index) {
            return Err(StreamInvariantError::ItemAlreadyCompleted);
        }
        if actual != expected {
            return Err(StreamInvariantError::ItemKindMismatch);
        }
        Ok(())
    }
}

fn kind_of(item: &ModelItem) -> ModelOutputKind {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamInvariantError {
    NonMonotonicStart,
    ItemNotStarted,
    ItemAlreadyCompleted,
    ItemKindMismatch,
}

impl StreamInvariantError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NonMonotonicStart => "non-monotonic-start",
            Self::ItemNotStarted => "item-not-started",
            Self::ItemAlreadyCompleted => "item-already-completed",
            Self::ItemKindMismatch => "item-kind-mismatch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserverDecision {
    Continue,
    Cancel,
}

pub trait ModelObserver {
    fn observe(&mut self, event: &ModelStreamEvent) -> ObserverDecision;
}

pub trait ModelCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelPortFailure {
    InvalidRequest,
    UnsupportedCapability,
    AdapterInvariant,
    RequiredPortFailure,
}

pub type ModelFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InvokeOutcome, ModelPortFailure>> + Send + 'a>>;

pub trait ModelPort: Send + Sync {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a>;
}
