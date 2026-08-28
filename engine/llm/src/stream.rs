use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};

use crate::{InvokeOutcome, ModelItem, ModelRequest, ModelUsage};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Expected semantic class of one indexed streaming output item.
pub enum ModelOutputKind {
    /// Generated text item.
    Text,
    /// Provider-declared refusal item.
    Refusal,
    /// Reasoning item or reference.
    Reasoning,
    /// Tool intent bound to its model-owned correlation identity.
    ToolIntent {
        /// Model call identity that every argument delta must match.
        model_call_id: String,
    },
    /// Tool observation item.
    ToolObservation,
    /// External media reference item.
    MediaReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Ordered provider-neutral progress event emitted during one invocation.
pub enum ModelStreamEvent {
    /// Declares a new monotonically indexed output item before deltas.
    OutputItemStarted {
        /// Provider-neutral output position.
        output_index: u32,
        /// Frozen semantic class for this index.
        kind: ModelOutputKind,
    },
    /// Appends UTF-8 text to a started text item.
    TextDelta {
        /// Target output position.
        output_index: u32,
        /// Ordered text fragment.
        delta: String,
    },
    /// Appends UTF-8 text to a started refusal item.
    RefusalDelta {
        /// Target output position.
        output_index: u32,
        /// Ordered refusal fragment.
        delta: String,
    },
    /// Appends UTF-8 text to a started reasoning item.
    ReasoningDelta {
        /// Target output position.
        output_index: u32,
        /// Ordered reasoning fragment admitted by visibility policy.
        delta: String,
    },
    /// Appends structured argument text to a started tool intent.
    ToolArgumentsDelta {
        /// Target output position.
        output_index: u32,
        /// Model call identity that must match the start event.
        model_call_id: String,
        /// Ordered arguments fragment.
        delta: String,
    },
    /// Closes one started item with its complete normalized value.
    OutputItemCompleted {
        /// Target output position.
        output_index: u32,
        /// Complete item whose kind must match its start event.
        item: ModelItem,
    },
    /// Replaces current invocation usage evidence with a newer snapshot.
    UsageUpdated {
        /// Normalized usage snapshot.
        usage: ModelUsage,
    },
}

#[derive(Default)]
/// Stateful validator for indexed stream ordering and item-kind invariants.
pub struct StreamValidator {
    started: BTreeMap<u32, ModelOutputKind>,
    completed: BTreeSet<u32>,
    last_started: Option<u32>,
}

impl StreamValidator {
    /// Applies one event, rejecting invalid ordering without silently repairing it.
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
/// Contract violation detected in a normalized model stream.
pub enum StreamInvariantError {
    /// Item starts did not use strictly increasing indices.
    NonMonotonicStart,
    /// A delta or completion referenced an index with no start event.
    ItemNotStarted,
    /// A delta or second completion arrived after the item completed.
    ItemAlreadyCompleted,
    /// Delta/completion semantics differ from the declared item kind.
    ItemKindMismatch,
}

impl StreamInvariantError {
    /// Returns the stable machine-readable invariant code.
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
/// Backpressure/cancellation decision returned by a stream observer.
pub enum ObserverDecision {
    /// Continue accepting invocation events.
    Continue,
    /// Ask the adapter to cooperatively cancel the invocation.
    Cancel,
}

/// Consumer of ordered normalized live events for one invocation.
pub trait ModelObserver: Send {
    /// Observes one event and returns whether dispatch should continue.
    fn observe(&mut self, event: &ModelStreamEvent) -> ObserverDecision;
}

/// Cooperative cancellation signal sampled by model adapters.
pub trait ModelCancellation: Send + Sync {
    /// Returns whether the enclosing execution requested cancellation.
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Failure of the model port contract rather than a valid invocation outcome.
pub enum ModelPortFailure {
    /// Request validation failed before dispatch.
    InvalidRequest,
    /// Selected adapter/target lacks a required capability.
    UnsupportedCapability,
    /// Adapter violated normalized request/stream/outcome invariants.
    AdapterInvariant,
    /// A required local port failed without a valid provider outcome.
    RequiredPortFailure,
}

/// Sendable borrowed future returned by [`ModelPort::invoke`].
pub type ModelFuture<'a> =
    Pin<Box<dyn Future<Output = Result<InvokeOutcome, ModelPortFailure>> + Send + 'a>>;

/// Provider-neutral asynchronous model invocation boundary.
pub trait ModelPort: Send + Sync {
    /// Validates/maps `request`, emits normalized events, and returns one outcome.
    ///
    /// Provider credentials and wire protocol details remain inside the adapter.
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a>;
}
