//! Bounded ephemeral publication of public Agent progress.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, Weak},
};

use garive_core::{AgentEvent, AgentEventKind, EventSink, PortFailure};
use garive_llm::{ModelOutputKind, ModelStreamEvent};
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Exact protocol version emitted by the H4 live-output boundary.
pub const LIVE_OUTPUT_API_VERSION: &str = "v1";

const MAX_PREVIEW_TEXT_BYTES: usize = 1_024 * 1_024;
const MAX_DELTA_TEXT_BYTES: usize = 32 * 1_024;

/// Explicit in-memory publication bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiveOutputLimits {
    /// Maximum simultaneously active execution previews.
    pub max_active_executions: usize,
    /// Maximum accumulated UTF-8 preview bytes per execution.
    pub max_preview_bytes: usize,
    /// Maximum UTF-8 text bytes in one published delta.
    pub max_event_bytes: usize,
    /// Per-Session broadcast queue capacity.
    pub broadcast_capacity: usize,
    /// Maximum concurrent subscribers for one Session.
    pub max_subscribers_per_session: usize,
}

/// Stable reason why an ephemeral publisher stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveOutputEndReason {
    /// Runtime committed a completed terminal transaction.
    TerminalCommitted,
    /// Runtime committed a suspension.
    Suspended,
    /// Runtime committed a stop.
    Stopped,
    /// Runtime committed a failure.
    Failed,
    /// The publisher closed without claiming a durable terminal.
    PublisherClosed,
}

/// One closed public live-output payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveOutputEventKind {
    /// Complete current in-memory preview for a reconnecting subscriber.
    Snapshot {
        /// Exact admitted text through `through_sequence`.
        text: String,
        /// Latest source sequence represented by `text`.
        through_sequence: u64,
    },
    /// Exact ordered suffix of the current public answer.
    TextDelta {
        /// Non-empty UTF-8 suffix.
        text: String,
    },
    /// Closed public execution phase.
    PhaseChanged {
        /// Stable phase code.
        phase: String,
        /// Stable localization key.
        label_key: String,
    },
    /// The complete preview can no longer be presented truthfully.
    PreviewUnavailable,
    /// Ephemeral publisher end; this is not durable terminal truth.
    Ended {
        /// Safe publisher-end classification.
        reason: LiveOutputEndReason,
    },
}

/// One bounded H4 semantic event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LiveOutputEvent {
    /// Exact H4 API version.
    pub api_version: &'static str,
    /// Owning Session identity.
    pub session_id: String,
    /// Owning Turn identity.
    pub turn_id: String,
    /// Owning Execution identity.
    pub execution_id: String,
    /// Fresh publisher generation UUID.
    pub stream_id: String,
    /// Monotonic sequence within the publisher generation.
    pub sequence: u64,
    /// Closed public payload.
    #[serde(flatten)]
    pub kind: LiveOutputEventKind,
}

/// Stable failure from H4 construction or publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveOutputError {
    /// One or more configured bounds are zero or inconsistent.
    InvalidLimits,
    /// The configured active-execution bound was reached.
    Capacity,
    /// The configured Session subscriber bound was reached.
    SubscribersFull,
    /// The requested execution is not active.
    UnknownExecution,
    /// Supplied Session or Turn ownership does not match the execution.
    IdentityMismatch,
    /// In-memory synchronization state was poisoned.
    Unavailable,
}

/// Stable receive outcome that cannot be represented as an event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveOutputReceiveError {
    /// Broadcast retention was exceeded; local preview must be discarded.
    Gap,
    /// The in-memory publisher channel closed.
    Closed,
}

/// Cloneable Runtime-owned live-output hub.
#[derive(Clone)]
pub struct LiveOutputHub {
    inner: Arc<Mutex<HubState>>,
    limits: LiveOutputLimits,
}

struct HubState {
    executions: BTreeMap<String, Generation>,
    sessions: BTreeMap<String, SessionChannel>,
}

struct SessionChannel {
    sender: broadcast::Sender<LiveOutputEvent>,
    subscribers: usize,
}

struct Generation {
    session_id: String,
    turn_id: String,
    execution_id: String,
    stream_id: String,
    sequence: u64,
    text: String,
    available: bool,
    phase: Option<&'static str>,
}

impl HubState {
    fn remove_inactive_session_channel(&mut self, session_id: &str) {
        let has_active_generation = self
            .executions
            .values()
            .any(|generation| generation.session_id == session_id);
        if !has_active_generation
            && self
                .sessions
                .get(session_id)
                .is_some_and(|channel| channel.subscribers == 0)
        {
            self.sessions.remove(session_id);
        }
    }
}

impl LiveOutputHub {
    /// Constructs an empty hub with explicit non-zero bounds.
    pub fn new(limits: LiveOutputLimits) -> Result<Self, LiveOutputError> {
        if limits.max_active_executions == 0
            || limits.max_preview_bytes == 0
            || limits.max_preview_bytes > MAX_PREVIEW_TEXT_BYTES
            || limits.max_event_bytes < 4
            || limits.max_event_bytes > MAX_DELTA_TEXT_BYTES
            || limits.broadcast_capacity == 0
            || limits.max_subscribers_per_session == 0
        {
            return Err(LiveOutputError::InvalidLimits);
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(HubState {
                executions: BTreeMap::new(),
                sessions: BTreeMap::new(),
            })),
            limits,
        })
    }

    /// Returns an Agent event sink backed by this hub.
    pub fn event_sink(&self) -> LiveOutputSink {
        LiveOutputSink { hub: self.clone() }
    }

    /// Subscribes to one Session and queues current active snapshots first.
    pub fn subscribe(&self, session_id: &str) -> Result<LiveOutputSubscriber, LiveOutputError> {
        if session_id.is_empty() {
            return Err(LiveOutputError::IdentityMismatch);
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|_| LiveOutputError::Unavailable)?;
        let channel = state
            .sessions
            .entry(session_id.to_owned())
            .or_insert_with(|| {
                let (sender, _) = broadcast::channel(self.limits.broadcast_capacity);
                SessionChannel {
                    sender,
                    subscribers: 0,
                }
            });
        if channel.subscribers >= self.limits.max_subscribers_per_session {
            return Err(LiveOutputError::SubscribersFull);
        }
        channel.subscribers += 1;
        let receiver = channel.sender.subscribe();
        let initial = state
            .executions
            .values()
            .filter(|generation| generation.session_id == session_id)
            .map(Generation::subscriber_snapshot)
            .collect();
        Ok(LiveOutputSubscriber {
            session_id: session_id.to_owned(),
            initial,
            receiver,
            hub: Arc::downgrade(&self.inner),
        })
    }

    /// Publishes a safe end marker and removes the active generation.
    pub fn end_execution(
        &self,
        session_id: &str,
        turn_id: &str,
        execution_id: &str,
        reason: LiveOutputEndReason,
    ) -> Result<(), LiveOutputError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| LiveOutputError::Unavailable)?;
        let mut generation = state
            .executions
            .remove(execution_id)
            .ok_or(LiveOutputError::UnknownExecution)?;
        if generation.session_id != session_id || generation.turn_id != turn_id {
            state.executions.insert(execution_id.to_owned(), generation);
            return Err(LiveOutputError::IdentityMismatch);
        }
        generation.sequence = generation.sequence.saturating_add(1);
        let event = generation.event(LiveOutputEventKind::Ended { reason });
        if let Some(channel) = state.sessions.get(session_id) {
            let _ = channel.sender.send(event);
        }
        state.remove_inactive_session_channel(session_id);
        Ok(())
    }

    fn publish(&self, event: AgentEvent) -> Result<(), LiveOutputError> {
        let action = match admitted_action(&event.kind) {
            Some(action) => action,
            None => return Ok(()),
        };
        let session_id = event.session_id.as_str().to_owned();
        let turn_id = event.turn_id.as_str().to_owned();
        let execution_id = event.execution_id.as_str().to_owned();
        let mut state = self
            .inner
            .lock()
            .map_err(|_| LiveOutputError::Unavailable)?;

        if !state.executions.contains_key(&execution_id) {
            if state.executions.len() >= self.limits.max_active_executions {
                return Err(LiveOutputError::Capacity);
            }
            state.executions.insert(
                execution_id.clone(),
                Generation::new(session_id.clone(), turn_id.clone(), execution_id.clone()),
            );
        }
        let sender = state
            .sessions
            .get(&session_id)
            .map(|channel| channel.sender.clone());
        let generation = state
            .executions
            .get_mut(&execution_id)
            .expect("generation inserted above");
        if generation.session_id != session_id || generation.turn_id != turn_id {
            return Err(LiveOutputError::IdentityMismatch);
        }
        let published = generation.apply(action, self.limits);
        if let Some(sender) = sender {
            for event in published {
                let _ = sender.send(event);
            }
        }
        Ok(())
    }
}

impl Generation {
    fn new(session_id: String, turn_id: String, execution_id: String) -> Self {
        Self {
            session_id,
            turn_id,
            execution_id,
            stream_id: Uuid::new_v4().hyphenated().to_string(),
            sequence: 0,
            text: String::new(),
            available: true,
            phase: None,
        }
    }

    fn apply(
        &mut self,
        action: AdmittedAction<'_>,
        limits: LiveOutputLimits,
    ) -> Vec<LiveOutputEvent> {
        match action {
            AdmittedAction::Phase(phase, label_key) => {
                if self.phase == Some(phase) {
                    return Vec::new();
                }
                self.phase = Some(phase);
                self.sequence = self.sequence.saturating_add(1);
                vec![self.event(LiveOutputEventKind::PhaseChanged {
                    phase: phase.into(),
                    label_key: label_key.into(),
                })]
            }
            AdmittedAction::Text(text) => self.append_text(text, limits),
        }
    }

    fn append_text(&mut self, text: &str, limits: LiveOutputLimits) -> Vec<LiveOutputEvent> {
        if text.is_empty() || !self.available {
            return Vec::new();
        }
        if self.text.len().saturating_add(text.len()) > limits.max_preview_bytes {
            self.text.clear();
            self.available = false;
            self.sequence = self.sequence.saturating_add(1);
            return vec![self.event(LiveOutputEventKind::PreviewUnavailable)];
        }
        let mut published = Vec::new();
        let mut remaining = text;
        while !remaining.is_empty() {
            let end = utf8_prefix(remaining, limits.max_event_bytes);
            let (chunk, rest) = remaining.split_at(end);
            self.text.push_str(chunk);
            self.sequence = self.sequence.saturating_add(1);
            published.push(self.event(LiveOutputEventKind::TextDelta { text: chunk.into() }));
            remaining = rest;
        }
        published
    }

    fn subscriber_snapshot(&self) -> LiveOutputEvent {
        let kind = if self.available {
            LiveOutputEventKind::Snapshot {
                text: self.text.clone(),
                through_sequence: self.sequence,
            }
        } else {
            LiveOutputEventKind::PreviewUnavailable
        };
        self.event(kind)
    }

    fn event(&self, kind: LiveOutputEventKind) -> LiveOutputEvent {
        LiveOutputEvent {
            api_version: LIVE_OUTPUT_API_VERSION,
            session_id: self.session_id.clone(),
            turn_id: self.turn_id.clone(),
            execution_id: self.execution_id.clone(),
            stream_id: self.stream_id.clone(),
            sequence: self.sequence,
            kind,
        }
    }
}

enum AdmittedAction<'a> {
    Phase(&'static str, &'static str),
    Text(&'a str),
}

fn admitted_action(kind: &AgentEventKind) -> Option<AdmittedAction<'_>> {
    match kind {
        AgentEventKind::ExecutionStarted | AgentEventKind::ContextDerived { .. } => {
            Some(AdmittedAction::Phase("preparing", "agent.live.preparing"))
        }
        AgentEventKind::ModelRequestPrepared { .. } => {
            Some(AdmittedAction::Phase("generating", "agent.live.generating"))
        }
        AgentEventKind::ModelStream(ModelStreamEvent::OutputItemStarted {
            kind: ModelOutputKind::Text,
            ..
        }) => Some(AdmittedAction::Phase("generating", "agent.live.generating")),
        AgentEventKind::ModelStream(ModelStreamEvent::TextDelta { delta, .. }) => {
            Some(AdmittedAction::Text(delta))
        }
        AgentEventKind::OutcomeProposed => {
            Some(AdmittedAction::Phase("finalizing", "agent.live.finalizing"))
        }
        _ => None,
    }
}

fn utf8_prefix(value: &str, max_bytes: usize) -> usize {
    if value.len() <= max_bytes {
        return value.len();
    }
    value
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= max_bytes)
        .last()
        .unwrap_or(value.len())
}

/// Agent event sink that publishes through a [`LiveOutputHub`].
pub struct LiveOutputSink {
    hub: LiveOutputHub,
}

impl EventSink for LiveOutputSink {
    fn emit(&mut self, event: AgentEvent) -> Result<(), PortFailure> {
        // H4 is deliberately lossy and must never fail durable execution.
        let _ = self.hub.publish(event);
        Ok(())
    }
}

/// One bounded Session subscription with reconnect snapshots queued first.
pub struct LiveOutputSubscriber {
    session_id: String,
    initial: VecDeque<LiveOutputEvent>,
    receiver: broadcast::Receiver<LiveOutputEvent>,
    hub: Weak<Mutex<HubState>>,
}

impl LiveOutputSubscriber {
    /// Receives one immediately available value without blocking.
    pub fn try_recv(&mut self) -> Result<Option<LiveOutputEvent>, LiveOutputReceiveError> {
        if let Some(event) = self.initial.pop_front() {
            return Ok(Some(event));
        }
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Lagged(_)) => Err(LiveOutputReceiveError::Gap),
            Err(broadcast::error::TryRecvError::Closed) => Err(LiveOutputReceiveError::Closed),
        }
    }

    /// Waits for the next value or an explicit gap/closed result.
    pub async fn recv(&mut self) -> Result<LiveOutputEvent, LiveOutputReceiveError> {
        if let Some(event) = self.initial.pop_front() {
            return Ok(event);
        }
        self.receiver.recv().await.map_err(|error| match error {
            broadcast::error::RecvError::Closed => LiveOutputReceiveError::Closed,
            broadcast::error::RecvError::Lagged(_) => LiveOutputReceiveError::Gap,
        })
    }
}

impl Drop for LiveOutputSubscriber {
    fn drop(&mut self) {
        let Some(hub) = self.hub.upgrade() else {
            return;
        };
        let Ok(mut state) = hub.lock() else {
            return;
        };
        let remove_channel = if let Some(channel) = state.sessions.get_mut(&self.session_id) {
            channel.subscribers = channel.subscribers.saturating_sub(1);
            channel.subscribers == 0
        } else {
            false
        };
        if remove_channel {
            state.sessions.remove(&self.session_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garive_core::{ExecutionId, SessionId, TurnId};

    fn limits() -> LiveOutputLimits {
        LiveOutputLimits {
            max_active_executions: 2,
            max_preview_bytes: 64,
            max_event_bytes: 16,
            broadcast_capacity: 4,
            max_subscribers_per_session: 2,
        }
    }

    fn started(session_id: &str, execution_id: &str) -> AgentEvent {
        AgentEvent {
            session_id: SessionId::try_from(session_id).unwrap(),
            turn_id: TurnId::try_from("turn-live").unwrap(),
            execution_id: ExecutionId::try_from(execution_id).unwrap(),
            kind: AgentEventKind::ExecutionStarted,
        }
    }

    #[test]
    fn session_channels_exist_only_while_subscribed() {
        let hub = LiveOutputHub::new(limits()).unwrap();
        let mut sink = hub.event_sink();

        sink.emit(started("session-unobserved", "execution-unobserved"))
            .unwrap();
        assert!(hub.inner.lock().unwrap().sessions.is_empty());

        hub.end_execution(
            "session-unobserved",
            "turn-live",
            "execution-unobserved",
            LiveOutputEndReason::TerminalCommitted,
        )
        .unwrap();
        assert!(hub.inner.lock().unwrap().sessions.is_empty());

        let (sender, _) = broadcast::channel(limits().broadcast_capacity);
        hub.inner.lock().unwrap().sessions.insert(
            "session-ending".into(),
            SessionChannel {
                sender,
                subscribers: 0,
            },
        );
        sink.emit(started("session-ending", "execution-ending"))
            .unwrap();
        hub.end_execution(
            "session-ending",
            "turn-live",
            "execution-ending",
            LiveOutputEndReason::TerminalCommitted,
        )
        .unwrap();
        assert!(hub.inner.lock().unwrap().sessions.is_empty());

        let subscriber = hub.subscribe("session-observed").unwrap();
        sink.emit(started("session-observed", "execution-observed"))
            .unwrap();
        hub.end_execution(
            "session-observed",
            "turn-live",
            "execution-observed",
            LiveOutputEndReason::TerminalCommitted,
        )
        .unwrap();
        assert_eq!(hub.inner.lock().unwrap().sessions.len(), 1);

        drop(subscriber);
        assert!(hub.inner.lock().unwrap().sessions.is_empty());
    }
}
