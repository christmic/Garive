use ratatui::text::{Line, Span};

use crate::{
    application::{LiveAnswer, LiveAnswerAvailability, LiveAnswerPhase},
    Theme,
};

use super::{
    conversation::live_cache::LiveRenderCache,
    palette,
    primitives::{LiveCaret, RoleMarker},
};

pub(super) fn render(
    answer: &LiveAnswer,
    theme: Theme,
    width: u16,
    reduced_motion: bool,
    cache: &mut LiveRenderCache,
) -> Vec<Line<'static>> {
    let colors = palette(theme);
    let mut lines = vec![Line::from(vec![
        RoleMarker::Agent.span(colors),
        Span::styled(phase_copy(answer, width), colors.muted),
    ])];
    match answer.availability {
        LiveAnswerAvailability::Unavailable => lines.push(Line::styled(
            "  Live feedback unavailable · waiting for saved result",
            colors.muted,
        )),
        LiveAnswerAvailability::Available => {
            if !answer.presented_text.is_empty() {
                lines.extend(cache.render_markdown(answer, theme, width));
                if let Some(line) = lines.last_mut() {
                    LiveCaret::for_output(
                        answer.availability == LiveAnswerAvailability::Available,
                        answer.ended,
                        reduced_motion,
                    )
                    .append_to(line, colors);
                }
            }
        }
    }
    lines.push(Line::default());
    lines
}

fn phase_copy(answer: &LiveAnswer, width: u16) -> &'static str {
    if answer.ended {
        if width < 52 {
            return "Awaiting saved result";
        }
        return "Waiting for durable result";
    }
    match answer.phase {
        Some(LiveAnswerPhase::Preparing) => "Preparing context",
        Some(LiveAnswerPhase::Generating) => "Writing…",
        Some(LiveAnswerPhase::Finalizing) => "Finishing…",
        None => "Working…",
    }
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
            false,
            &mut cache,
        );
        let awaiting_text = line_text(&awaiting);
        assert_eq!(awaiting_text.len(), 2, "status plus turn gap");
        assert!(awaiting_text[0].contains("Preparing context"));
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
            false,
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
            true,
            &mut cache,
        );
        assert_eq!(line_text(&lines)[0], "• Awaiting saved result");
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
