use ratatui::text::Line;

use crate::{application::LiveAnswer, Theme};

use super::{
    conversation::live_cache::LiveRenderCache,
    palette,
    primitives::{LiveCaret, RoleMarker},
    MotionFrame,
};

pub(super) fn render(
    answer: &LiveAnswer,
    theme: Theme,
    width: u16,
    motion: MotionFrame,
    cache: &mut LiveRenderCache,
) -> Vec<Line<'static>> {
    let colors = palette(theme);
    let mut lines = if answer.presented_text.is_empty() {
        Vec::new()
    } else {
        let mut rendered = cache.render_markdown(answer, theme, width);
        if let Some(first) = rendered.first_mut() {
            if first
                .spans
                .first()
                .is_some_and(|span| span.content.as_ref() == "  ")
            {
                first.spans.remove(0);
            }
            RoleMarker::Agent.prepend_to(first, colors);
        }
        if let Some(last) = rendered.last_mut() {
            LiveCaret::for_output(true, answer.ended, motion).append_to(last, colors);
        }
        rendered
    };
    lines.push(Line::default());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{LiveAnswerExpectation, LiveAnswerProjection};
    use garive_host_client::{LiveOutputEvent, LiveOutputEventKind};

    #[test]
    fn awaiting_first_delta_has_status_without_orphan_caret() {
        let mut projection = LiveAnswerProjection::default();
        projection.apply(
            event(
                1,
                LiveOutputEventKind::Snapshot {
                    text: String::new(),
                    through_sequence: 1,
                },
            ),
            expectation(),
        );
        projection.apply(
            event(
                2,
                LiveOutputEventKind::PhaseChanged {
                    phase: "preparing".into(),
                    label_key: "agent.live.preparing".into(),
                },
            ),
            expectation(),
        );

        let mut cache = LiveRenderCache::default();
        let awaiting = render(
            projection.current().unwrap(),
            Theme::Dark,
            40,
            MotionFrame::animated(0),
            &mut cache,
        );
        let awaiting_text = line_text(&awaiting);
        assert_eq!(awaiting_text.len(), 1, "only the turn gap remains");
        assert!(!awaiting_text[0].contains("Preparing context"));
        assert!(!awaiting_text.join("\n").contains('▍'));

        projection.apply(
            event(3, LiveOutputEventKind::TextDelta { text: "A".into() }),
            expectation(),
        );
        projection.advance_frame(false);
        let visible = render(
            projection.current().unwrap(),
            Theme::Dark,
            40,
            MotionFrame::animated(0),
            &mut cache,
        );
        assert_eq!(line_text(&visible).join("\n").matches('▍').count(), 1);
    }

    #[test]
    fn ended_phase_uses_bounded_compact_copy() {
        let mut projection = LiveAnswerProjection::default();
        projection.apply(
            event(
                1,
                LiveOutputEventKind::Snapshot {
                    text: "Saved preview".into(),
                    through_sequence: 1,
                },
            ),
            expectation(),
        );
        projection.apply(
            event(
                2,
                LiveOutputEventKind::Ended {
                    reason: garive_host_client::LiveOutputEndReason::TerminalCommitted,
                },
            ),
            expectation(),
        );

        let mut cache = LiveRenderCache::default();
        let lines = render(
            projection.current().unwrap(),
            Theme::Mono,
            36,
            MotionFrame::reduced(),
            &mut cache,
        );
        assert_eq!(line_text(&lines)[0], "• Saved preview");
        assert!(unicode_width::UnicodeWidthStr::width(line_text(&lines)[0].as_str()) <= 36);
    }

    fn line_text(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    fn event(sequence: u64, kind: LiveOutputEventKind) -> LiveOutputEvent {
        LiveOutputEvent {
            api_version: "v1".into(),
            session_id: "session-a".into(),
            turn_id: "turn-a".into(),
            execution_id: "execution-a".into(),
            stream_id: "00000000-0000-4000-8000-000000000001".into(),
            sequence,
            kind,
        }
    }

    fn expectation() -> LiveAnswerExpectation<'static> {
        LiveAnswerExpectation {
            selected_session: "session-a",
            active_turn: Some("turn-a"),
            active_execution: Some("execution-a"),
            detached: false,
        }
    }
}
