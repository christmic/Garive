use garive_host_client::{LiveOutputEvent, LiveOutputEventKind};
use pulldown_cmark::{Event as MarkdownEvent, Options, Parser};

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
    pub(crate) markdown: LiveMarkdownBuffer,
    pub(crate) phase: Option<LiveAnswerPhase>,
    pub(crate) last_sequence: u64,
    pub(crate) availability: LiveAnswerAvailability,
    pub(crate) ended: bool,
}

impl LiveAnswer {
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
            markdown: LiveMarkdownBuffer::default(),
            phase: None,
            last_sequence: 0,
            availability: LiveAnswerAvailability::Available,
            ended: false,
        }
    }

    const fn unseen(detached: bool) -> bool {
        detached
    }

    fn present(&mut self, text: String) {
        self.markdown.update(&text);
        self.presented_text = text;
    }

    fn clear_preview(&mut self) {
        self.received_text.clear();
        self.presented_text.clear();
        self.markdown.clear();
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LiveMarkdownBuffer {
    stable_prefix: String,
    mutable_tail: String,
}

impl LiveMarkdownBuffer {
    pub(crate) fn stable_prefix(&self) -> &str {
        &self.stable_prefix
    }

    pub(crate) fn mutable_tail(&self) -> &str {
        &self.mutable_tail
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn as_text(&self) -> String {
        let mut text = String::with_capacity(self.stable_prefix.len() + self.mutable_tail.len());
        text.push_str(&self.stable_prefix);
        text.push_str(&self.mutable_tail);
        text
    }

    fn update(&mut self, text: &str) {
        let boundary = stable_markdown_boundary(text);
        let (stable, tail) = text.split_at(boundary);
        self.stable_prefix.clear();
        self.stable_prefix.push_str(stable);
        self.mutable_tail.clear();
        self.mutable_tail.push_str(tail);
    }

    fn clear(&mut self) {
        self.stable_prefix.clear();
        self.mutable_tail.clear();
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
                answer.present(text);
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
                answer.clear_preview();
                answer.availability = LiveAnswerAvailability::Unavailable;
                effect.visual_changed = changed;
            }
            LiveOutputEventKind::Ended { .. } => {
                if answer.presented_text != answer.received_text {
                    answer.present(answer.received_text.clone());
                }
                answer.ended = true;
                effect.visual_changed = true;
            }
        }
        if effect.visual_changed {
            effect.unseen_increment = LiveAnswer::unseen(expectation.detached);
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
        answer.present(answer.received_text.clone());
        LiveAnswerEffect {
            accepted: true,
            visual_changed: true,
            frame_requested: false,
            unseen_increment: LiveAnswer::unseen(detached),
        }
    }

    pub(crate) fn mark_seen(&mut self) {}

    pub(crate) fn preview_unavailable(&mut self, detached: bool) -> LiveAnswerEffect {
        let Some(answer) = self.current.as_mut() else {
            return LiveAnswerEffect::default();
        };
        let changed = answer.availability != LiveAnswerAvailability::Unavailable
            || !answer.received_text.is_empty()
            || !answer.presented_text.is_empty();
        answer.clear_preview();
        answer.availability = LiveAnswerAvailability::Unavailable;
        LiveAnswerEffect {
            accepted: true,
            visual_changed: changed,
            frame_requested: false,
            unseen_increment: changed && LiveAnswer::unseen(detached),
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

    pub(crate) fn await_durable_snapshot(
        &mut self,
        session_id: &str,
        turn_id: &str,
        execution_id: Option<&str>,
    ) {
        if let Some(answer) = self.current.as_mut().filter(|answer| {
            answer.key.session_id == session_id
                && answer.key.turn_id == turn_id
                && execution_id.is_none_or(|execution| answer.key.execution_id == execution)
        }) {
            if answer.presented_text != answer.received_text {
                answer.present(answer.received_text.clone());
            }
            answer.ended = true;
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

fn stable_markdown_boundary(source: &str) -> usize {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options);
    if parser.reference_definitions().iter().next().is_some() {
        return 0;
    }
    let mut depth = 0_usize;
    let mut latest_top_level = 0;
    for (event, range) in parser.into_offset_iter() {
        match event {
            MarkdownEvent::Start(_) => {
                if depth == 0 {
                    latest_top_level = range.start;
                }
                depth = depth.saturating_add(1);
            }
            MarkdownEvent::End(_) => depth = depth.saturating_sub(1),
            MarkdownEvent::Rule if depth == 0 => latest_top_level = range.start,
            _ => {}
        }
    }
    latest_top_level
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
