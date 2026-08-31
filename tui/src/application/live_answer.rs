use garive_host_client::{LiveOutputEvent, LiveOutputEventKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveAnswerKey {
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) execution_id: String,
    pub(crate) stream_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiveAnswerPhase {
    Preparing,
    Generating,
    Finalizing,
}

impl LiveAnswerPhase {
    fn from_wire(value: &str) -> Option<Self> {
        match value {
            "preparing" => Some(Self::Preparing),
            "generating" => Some(Self::Generating),
            "finalizing" => Some(Self::Finalizing),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiveAnswerAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LiveAnswerEffect {
    pub(crate) accepted: bool,
    pub(crate) visual_changed: bool,
    pub(crate) frame_requested: bool,
    pub(crate) unseen_increment: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LiveAnswerExpectation<'a> {
    pub(crate) selected_session: &'a str,
    pub(crate) active_turn: Option<&'a str>,
    pub(crate) active_execution: Option<&'a str>,
    pub(crate) detached: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveAnswer {
    pub(crate) key: LiveAnswerKey,
    pub(crate) received_text: String,
    pub(crate) presented_text: String,
    pub(crate) phase: Option<LiveAnswerPhase>,
    pub(crate) last_sequence: u64,
    pub(crate) availability: LiveAnswerAvailability,
    pub(crate) ended: bool,
    unseen_notified: bool,
}

impl LiveAnswer {
    pub(crate) const fn caret_visible(&self) -> bool {
        !self.ended && matches!(self.availability, LiveAnswerAvailability::Available)
    }

    fn from_initial(event: &LiveOutputEvent) -> Self {
        Self {
            key: LiveAnswerKey {
                session_id: event.session_id.clone(),
                turn_id: event.turn_id.clone(),
                execution_id: event.execution_id.clone(),
                stream_id: event.stream_id.clone(),
            },
            received_text: String::new(),
            presented_text: String::new(),
            phase: None,
            last_sequence: 0,
            availability: LiveAnswerAvailability::Available,
            ended: false,
            unseen_notified: false,
        }
    }

    fn mark_unseen(&mut self, detached: bool) -> bool {
        if !detached {
            self.unseen_notified = false;
            return false;
        }
        if self.unseen_notified {
            return false;
        }
        self.unseen_notified = true;
        true
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LiveAnswerProjection {
    current: Option<LiveAnswer>,
    durable_fence: Option<(String, String, Option<String>)>,
    retired_stream: Option<String>,
}

impl LiveAnswerProjection {
    pub(crate) fn current(&self) -> Option<&LiveAnswer> {
        self.current.as_ref()
    }

    pub(crate) fn frame_pending(&self) -> bool {
        self.current.as_ref().is_some_and(|answer| {
            answer.availability == LiveAnswerAvailability::Available
                && answer.presented_text != answer.received_text
        })
    }

    pub(crate) fn apply(
        &mut self,
        event: LiveOutputEvent,
        expectation: LiveAnswerExpectation<'_>,
    ) -> LiveAnswerEffect {
        if !valid_event(&event)
            || event.session_id != expectation.selected_session
            || expectation.active_turn != Some(event.turn_id.as_str())
            || expectation
                .active_execution
                .is_some_and(|execution| execution != event.execution_id)
            || self
                .durable_fence
                .as_ref()
                .is_some_and(|(session, turn, execution)| {
                    session == &event.session_id
                        && turn == &event.turn_id
                        && execution
                            .as_deref()
                            .is_none_or(|value| value == event.execution_id)
                })
            || self
                .retired_stream
                .as_ref()
                .is_some_and(|stream| stream == &event.stream_id)
        {
            return LiveAnswerEffect::default();
        }

        let same_stream = self.current.as_ref().is_some_and(|answer| {
            answer.key.session_id == event.session_id
                && answer.key.turn_id == event.turn_id
                && answer.key.execution_id == event.execution_id
                && answer.key.stream_id == event.stream_id
        });
        if !same_stream {
            if !is_initial_event(&event) {
                return LiveAnswerEffect::default();
            }
            if let Some(previous) = self.current.take() {
                self.retired_stream = Some(previous.key.stream_id);
            }
            self.current = Some(LiveAnswer::from_initial(&event));
        }

        let answer = self.current.as_mut().expect("initial event created state");
        if answer.ended {
            return LiveAnswerEffect::default();
        }

        let is_checkpoint = matches!(event.kind, LiveOutputEventKind::Snapshot { .. })
            || (!same_stream && matches!(event.kind, LiveOutputEventKind::PreviewUnavailable));
        if is_checkpoint {
            if event.sequence < answer.last_sequence {
                return LiveAnswerEffect::default();
            }
        } else if event.sequence != answer.last_sequence.saturating_add(1) {
            return LiveAnswerEffect::default();
        }

        let mut effect = LiveAnswerEffect {
            accepted: true,
            ..LiveAnswerEffect::default()
        };
        answer.last_sequence = event.sequence;
        match event.kind {
            LiveOutputEventKind::Snapshot {
                text,
                through_sequence: _,
            } => {
                let changed = answer.received_text != text
                    || answer.presented_text != text
                    || answer.availability != LiveAnswerAvailability::Available;
                answer.received_text = text.clone();
                answer.presented_text = text;
                answer.availability = LiveAnswerAvailability::Available;
                effect.visual_changed = changed;
            }
            LiveOutputEventKind::TextDelta { text } => {
                if answer.availability == LiveAnswerAvailability::Available {
                    answer.received_text.push_str(&text);
                    effect.frame_requested = true;
                }
            }
            LiveOutputEventKind::PhaseChanged { phase, .. } => {
                let Some(phase) = LiveAnswerPhase::from_wire(&phase) else {
                    return LiveAnswerEffect::default();
                };
                effect.visual_changed = answer.phase != Some(phase);
                answer.phase = Some(phase);
            }
            LiveOutputEventKind::PreviewUnavailable => {
                let changed = answer.availability != LiveAnswerAvailability::Unavailable
                    || !answer.received_text.is_empty()
                    || !answer.presented_text.is_empty();
                answer.received_text.clear();
                answer.presented_text.clear();
                answer.availability = LiveAnswerAvailability::Unavailable;
                effect.visual_changed = changed;
            }
            LiveOutputEventKind::Ended { .. } => {
                answer.ended = true;
                effect.visual_changed = true;
            }
        }
        if effect.visual_changed {
            effect.unseen_increment = answer.mark_unseen(expectation.detached);
        }
        effect
    }

    pub(crate) fn advance_frame(&mut self, detached: bool) -> LiveAnswerEffect {
        let Some(answer) = self.current.as_mut() else {
            return LiveAnswerEffect::default();
        };
        if answer.availability != LiveAnswerAvailability::Available
            || answer.presented_text == answer.received_text
        {
            return LiveAnswerEffect::default();
        }
        answer.presented_text.clone_from(&answer.received_text);
        LiveAnswerEffect {
            accepted: true,
            visual_changed: true,
            frame_requested: false,
            unseen_increment: answer.mark_unseen(detached),
        }
    }

    pub(crate) fn mark_seen(&mut self) {
        if let Some(answer) = self.current.as_mut() {
            answer.unseen_notified = false;
        }
    }

    pub(crate) fn preview_unavailable(&mut self, detached: bool) -> LiveAnswerEffect {
        let Some(answer) = self.current.as_mut() else {
            return LiveAnswerEffect::default();
        };
        let changed = answer.availability != LiveAnswerAvailability::Unavailable
            || !answer.received_text.is_empty()
            || !answer.presented_text.is_empty();
        answer.received_text.clear();
        answer.presented_text.clear();
        answer.availability = LiveAnswerAvailability::Unavailable;
        LiveAnswerEffect {
            accepted: true,
            visual_changed: changed,
            frame_requested: false,
            unseen_increment: changed && answer.mark_unseen(detached),
        }
    }

    pub(crate) fn durable_takeover(
        &mut self,
        session_id: &str,
        turn_id: &str,
        execution_id: Option<&str>,
    ) {
        if self.current.as_ref().is_some_and(|answer| {
            answer.key.session_id == session_id
                && answer.key.turn_id == turn_id
                && execution_id.is_none_or(|execution| answer.key.execution_id == execution)
        }) {
            if let Some(previous) = self.current.take() {
                self.retired_stream = Some(previous.key.stream_id);
            }
        }
        self.durable_fence = Some((
            session_id.to_owned(),
            turn_id.to_owned(),
            execution_id.map(str::to_owned),
        ));
    }

    pub(crate) fn clear_for_session_change(&mut self) {
        self.current = None;
        self.durable_fence = None;
        self.retired_stream = None;
    }
}

fn is_initial_event(event: &LiveOutputEvent) -> bool {
    event.sequence == 1
        || matches!(
            event.kind,
            LiveOutputEventKind::Snapshot { .. } | LiveOutputEventKind::PreviewUnavailable
        )
}

fn valid_event(event: &LiveOutputEvent) -> bool {
    if event.sequence == 0 {
        return false;
    }
    match &event.kind {
        LiveOutputEventKind::Snapshot {
            through_sequence, ..
        } => *through_sequence == event.sequence,
        LiveOutputEventKind::TextDelta { text } => !text.is_empty(),
        LiveOutputEventKind::PhaseChanged { phase, label_key } => matches!(
            (phase.as_str(), label_key.as_str()),
            ("preparing", "agent.live.preparing")
                | ("generating", "agent.live.generating")
                | ("finalizing", "agent.live.finalizing")
        ),
        LiveOutputEventKind::PreviewUnavailable | LiveOutputEventKind::Ended { .. } => true,
    }
}
