#![allow(dead_code, unused_imports)]

#[path = "../src/args.rs"]
mod args;
pub use args::{MouseMode, Theme};
#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;

use application::{
    LiveAnswerAvailability, LiveAnswerExpectation, LiveAnswerPhase, LiveAnswerProjection,
};
use garive_host_client::LiveOutputEndReason;
use garive_host_client::{LiveOutputEvent, LiveOutputEventKind};

const STREAM_A: &str = "00000000-0000-4000-8000-000000000001";
const STREAM_B: &str = "00000000-0000-4000-8000-000000000002";

fn event(stream: &str, sequence: u64, kind: LiveOutputEventKind) -> LiveOutputEvent {
    LiveOutputEvent {
        api_version: "v1".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
        execution_id: "execution-a".into(),
        stream_id: stream.into(),
        sequence,
        kind,
    }
}

fn expectation(detached: bool) -> LiveAnswerExpectation<'static> {
    LiveAnswerExpectation {
        selected_session: "session-a",
        active_turn: Some("turn-a"),
        active_execution: Some("execution-a"),
        detached,
    }
}

fn snapshot(stream: &str, sequence: u64, text: &str) -> LiveOutputEvent {
    event(
        stream,
        sequence,
        LiveOutputEventKind::Snapshot {
            text: text.into(),
            through_sequence: sequence,
        },
    )
}

#[test]
fn snapshot_atomically_installs_received_and_presented_text() {
    let mut projection = LiveAnswerProjection::default();
    let effect = projection.apply(
        snapshot(STREAM_A, 4, "complete preview"),
        expectation(false),
    );

    let answer = projection.current().unwrap();
    assert!(effect.accepted && effect.visual_changed);
    assert_eq!(answer.received_text, "complete preview");
    assert_eq!(answer.presented_text, "complete preview");
    assert_eq!(answer.last_sequence, 4);
    assert!(!projection.frame_pending());
}

#[test]
fn contiguous_delta_is_received_then_presented_in_one_frame() {
    let mut projection = LiveAnswerProjection::default();
    projection.apply(snapshot(STREAM_A, 1, "hello"), expectation(false));
    let delta = projection.apply(
        event(
            STREAM_A,
            2,
            LiveOutputEventKind::TextDelta {
                text: " 世界".into(),
            },
        ),
        expectation(false),
    );

    let answer = projection.current().unwrap();
    assert!(delta.accepted && delta.frame_requested && !delta.visual_changed);
    assert_eq!(answer.received_text, "hello 世界");
    assert_eq!(answer.presented_text, "hello");
    assert!(projection.frame_pending());

    let frame = projection.advance_frame(false);
    assert!(frame.visual_changed);
    assert_eq!(projection.current().unwrap().presented_text, "hello 世界");
    assert!(!projection.frame_pending());
    assert!(!projection.advance_frame(false).visual_changed);
}

#[test]
fn phase_then_unavailable_clears_all_untrusted_preview() {
    let mut projection = LiveAnswerProjection::default();
    projection.apply(
        event(
            STREAM_A,
            1,
            LiveOutputEventKind::PhaseChanged {
                phase: "generating".into(),
                label_key: "agent.live.generating".into(),
            },
        ),
        expectation(false),
    );
    projection.apply(
        event(
            STREAM_A,
            2,
            LiveOutputEventKind::TextDelta {
                text: "incomplete".into(),
            },
        ),
        expectation(false),
    );
    projection.advance_frame(false);

    let effect = projection.apply(
        event(STREAM_A, 3, LiveOutputEventKind::PreviewUnavailable),
        expectation(false),
    );
    let answer = projection.current().unwrap();
    assert!(effect.visual_changed);
    assert_eq!(answer.phase, Some(LiveAnswerPhase::Generating));
    assert_eq!(answer.availability, LiveAnswerAvailability::Unavailable);
    assert!(answer.received_text.is_empty() && answer.presented_text.is_empty());
    assert!(!answer.caret_visible());
}

#[test]
fn ended_only_closes_caret_and_waits_for_durable_truth() {
    let mut projection = LiveAnswerProjection::default();
    projection.apply(snapshot(STREAM_A, 1, "ephemeral"), expectation(false));
    let effect = projection.apply(
        event(
            STREAM_A,
            2,
            LiveOutputEventKind::Ended {
                reason: LiveOutputEndReason::TerminalCommitted,
            },
        ),
        expectation(false),
    );

    let answer = projection.current().unwrap();
    assert!(effect.accepted && effect.visual_changed);
    assert!(answer.ended && !answer.caret_visible());
    assert_eq!(answer.presented_text, "ephemeral");
    assert!(projection.current().is_some());

    let late = projection.apply(
        event(
            STREAM_A,
            3,
            LiveOutputEventKind::TextDelta {
                text: " late".into(),
            },
        ),
        expectation(false),
    );
    assert!(!late.accepted);
}

#[test]
fn new_generation_replaces_once_and_retired_stream_cannot_return() {
    let mut projection = LiveAnswerProjection::default();
    projection.apply(snapshot(STREAM_A, 8, "old"), expectation(false));
    assert!(
        projection
            .apply(snapshot(STREAM_B, 1, "new"), expectation(false))
            .accepted
    );
    assert_eq!(projection.current().unwrap().key.stream_id, STREAM_B);

    let retired = projection.apply(snapshot(STREAM_A, 9, "stale"), expectation(false));
    assert!(!retired.accepted);
    assert_eq!(projection.current().unwrap().presented_text, "new");

    let wrong_identity = LiveAnswerExpectation {
        selected_session: "session-other",
        ..expectation(false)
    };
    assert!(
        !projection
            .apply(
                event(
                    STREAM_B,
                    2,
                    LiveOutputEventKind::TextDelta {
                        text: " wrong".into(),
                    },
                ),
                wrong_identity,
            )
            .accepted
    );
}

#[test]
fn durable_takeover_removes_preview_and_fences_late_generation() {
    let mut projection = LiveAnswerProjection::default();
    projection.apply(snapshot(STREAM_A, 1, "preview"), expectation(false));
    projection.durable_takeover("session-a", "turn-a", Some("execution-a"));
    assert!(projection.current().is_none());

    let resurrect = projection.apply(
        event(
            STREAM_B,
            1,
            LiveOutputEventKind::TextDelta {
                text: "resurrect".into(),
            },
        ),
        expectation(false),
    );
    assert!(!resurrect.accepted && projection.current().is_none());
}

#[test]
fn detached_view_increments_unseen_once_until_marked_seen() {
    let mut projection = LiveAnswerProjection::default();
    let first = projection.apply(snapshot(STREAM_A, 1, "one"), expectation(true));
    assert!(first.unseen_increment);

    projection.apply(
        event(
            STREAM_A,
            2,
            LiveOutputEventKind::TextDelta {
                text: " two".into(),
            },
        ),
        expectation(true),
    );
    assert!(!projection.advance_frame(true).unseen_increment);

    projection.mark_seen();
    projection.apply(
        event(
            STREAM_A,
            3,
            LiveOutputEventKind::TextDelta {
                text: " three".into(),
            },
        ),
        expectation(true),
    );
    assert!(projection.advance_frame(true).unseen_increment);
}
