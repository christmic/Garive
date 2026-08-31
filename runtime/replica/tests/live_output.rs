use garive_core::{AgentEvent, AgentEventKind, EventSink, ExecutionId, SessionId, TurnId};
use garive_llm::{ModelOutputKind, ModelStreamEvent};
use garive_runtime::{
    LiveOutputEndReason, LiveOutputEventKind, LiveOutputHub, LiveOutputLimits,
    LiveOutputReceiveError,
};

fn limits() -> LiveOutputLimits {
    LiveOutputLimits {
        max_active_executions: 4,
        max_preview_bytes: 64,
        max_event_bytes: 16,
        broadcast_capacity: 8,
        max_subscribers_per_session: 2,
    }
}

fn event(kind: AgentEventKind) -> AgentEvent {
    AgentEvent {
        session_id: SessionId::try_from("session-live").unwrap(),
        turn_id: TurnId::try_from("turn-live").unwrap(),
        execution_id: ExecutionId::try_from("execution-live").unwrap(),
        kind,
    }
}

#[test]
fn limits_above_h4_wire_contract_are_rejected() {
    assert!(matches!(
        LiveOutputHub::new(LiveOutputLimits {
            max_preview_bytes: 1_024 * 1_024 + 1,
            ..limits()
        }),
        Err(garive_runtime::LiveOutputError::InvalidLimits)
    ));
    assert!(matches!(
        LiveOutputHub::new(LiveOutputLimits {
            max_event_bytes: 32 * 1_024 + 1,
            ..limits()
        }),
        Err(garive_runtime::LiveOutputError::InvalidLimits)
    ));
}

#[test]
fn publishes_only_safe_public_progress_in_exact_order() {
    let hub = LiveOutputHub::new(limits()).unwrap();
    let mut subscriber = hub.subscribe("session-live").unwrap();
    let mut sink = hub.event_sink();

    sink.emit(event(AgentEventKind::ExecutionStarted)).unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::OutputItemStarted {
            output_index: 0,
            kind: ModelOutputKind::Text,
        },
    )))
    .unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "hello".into(),
        },
    )))
    .unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::ReasoningDelta {
            output_index: 1,
            delta: "private reasoning".into(),
        },
    )))
    .unwrap();
    sink.emit(event(AgentEventKind::OutcomeProposed)).unwrap();

    let preparing = subscriber.try_recv().unwrap().unwrap();
    let generating = subscriber.try_recv().unwrap().unwrap();
    let delta = subscriber.try_recv().unwrap().unwrap();
    let finalizing = subscriber.try_recv().unwrap().unwrap();

    assert_eq!(preparing.sequence, 1);
    assert!(matches!(
        preparing.kind,
        LiveOutputEventKind::PhaseChanged { ref phase, .. } if phase == "preparing"
    ));
    assert_eq!(generating.sequence, 2);
    assert!(matches!(
        generating.kind,
        LiveOutputEventKind::PhaseChanged { ref phase, .. } if phase == "generating"
    ));
    assert_eq!(delta.sequence, 3);
    assert!(matches!(
        delta.kind,
        LiveOutputEventKind::TextDelta { ref text } if text == "hello"
    ));
    assert_eq!(finalizing.sequence, 4);
    assert!(matches!(
        finalizing.kind,
        LiveOutputEventKind::PhaseChanged { ref phase, .. } if phase == "finalizing"
    ));
    assert!(subscriber.try_recv().unwrap().is_none());
}

#[test]
fn reconnect_starts_with_complete_current_snapshot() {
    let hub = LiveOutputHub::new(limits()).unwrap();
    let mut sink = hub.event_sink();
    sink.emit(event(AgentEventKind::ExecutionStarted)).unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "hello".into(),
        },
    )))
    .unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: " world".into(),
        },
    )))
    .unwrap();

    let mut subscriber = hub.subscribe("session-live").unwrap();
    let snapshot = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(snapshot.sequence, 3);
    assert!(matches!(
        snapshot.kind,
        LiveOutputEventKind::Snapshot {
            ref text,
            through_sequence: 3
        } if text == "hello world"
    ));
}

#[test]
fn reconnect_after_last_subscriber_drops_uses_generation_snapshot() {
    let hub = LiveOutputHub::new(limits()).unwrap();
    let subscriber = hub.subscribe("session-live").unwrap();
    let mut sink = hub.event_sink();
    sink.emit(event(AgentEventKind::ExecutionStarted)).unwrap();
    drop(subscriber);

    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "after disconnect".into(),
        },
    )))
    .unwrap();

    let mut reconnected = hub.subscribe("session-live").unwrap();
    let snapshot = reconnected.try_recv().unwrap().unwrap();
    assert_eq!(snapshot.sequence, 2);
    assert!(matches!(
        snapshot.kind,
        LiveOutputEventKind::Snapshot {
            ref text,
            through_sequence: 2
        } if text == "after disconnect"
    ));
}

#[test]
fn preview_overflow_clears_text_and_stays_visibly_unavailable() {
    let hub = LiveOutputHub::new(LiveOutputLimits {
        max_preview_bytes: 5,
        ..limits()
    })
    .unwrap();
    let mut sink = hub.event_sink();
    sink.emit(event(AgentEventKind::ExecutionStarted)).unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "hello".into(),
        },
    )))
    .unwrap();
    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "!".into(),
        },
    )))
    .unwrap();

    let mut subscriber = hub.subscribe("session-live").unwrap();
    let unavailable = subscriber.try_recv().unwrap().unwrap();
    assert_eq!(unavailable.sequence, 3);
    assert!(matches!(
        unavailable.kind,
        LiveOutputEventKind::PreviewUnavailable
    ));

    sink.emit(event(AgentEventKind::ModelStream(
        ModelStreamEvent::TextDelta {
            output_index: 0,
            delta: "later".into(),
        },
    )))
    .unwrap();
    assert!(subscriber.try_recv().unwrap().is_none());
}

#[test]
fn lag_is_reported_instead_of_silently_skipping_sequences() {
    let hub = LiveOutputHub::new(LiveOutputLimits {
        broadcast_capacity: 2,
        ..limits()
    })
    .unwrap();
    let mut subscriber = hub.subscribe("session-live").unwrap();
    let mut sink = hub.event_sink();
    sink.emit(event(AgentEventKind::ExecutionStarted)).unwrap();
    sink.emit(event(AgentEventKind::ContextDerived {
        item_count: 1,
        utf8_bytes: 1,
    }))
    .unwrap();
    sink.emit(event(AgentEventKind::ModelRequestPrepared {
        request_id: "private-request".into(),
        target_id: "private-target".into(),
    }))
    .unwrap();
    sink.emit(event(AgentEventKind::OutcomeProposed)).unwrap();

    assert_eq!(subscriber.try_recv(), Err(LiveOutputReceiveError::Gap));
}

#[test]
fn end_is_ephemeral_and_does_not_claim_a_durable_terminal() {
    let hub = LiveOutputHub::new(limits()).unwrap();
    let mut subscriber = hub.subscribe("session-live").unwrap();
    let mut sink = hub.event_sink();
    sink.emit(event(AgentEventKind::ExecutionStarted)).unwrap();
    hub.end_execution(
        "session-live",
        "turn-live",
        "execution-live",
        LiveOutputEndReason::TerminalCommitted,
    )
    .unwrap();

    subscriber.try_recv().unwrap().unwrap();
    let ended = subscriber.try_recv().unwrap().unwrap();
    assert!(matches!(
        ended.kind,
        LiveOutputEventKind::Ended {
            reason: LiveOutputEndReason::TerminalCommitted
        }
    ));
    assert!(hub
        .subscribe("session-live")
        .unwrap()
        .try_recv()
        .unwrap()
        .is_none());
}
