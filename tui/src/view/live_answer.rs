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
        Span::styled(phase_copy(answer), colors.muted),
    ])];
    match answer.availability {
        LiveAnswerAvailability::Unavailable => lines.push(Line::styled(
            "  Live feedback unavailable · waiting for saved result",
            colors.muted,
        )),
        LiveAnswerAvailability::Available => {
            if answer.presented_text.is_empty() {
                lines.push(Line::styled("  ", colors.normal));
            } else {
                lines.extend(cache.render_markdown(answer, theme, width));
            }
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
    lines.push(Line::default());
    lines
}

fn phase_copy(answer: &LiveAnswer) -> &'static str {
    if answer.ended {
        return " · Waiting for durable result";
    }
    match answer.phase {
        Some(LiveAnswerPhase::Preparing) => " · Preparing context",
        Some(LiveAnswerPhase::Generating) => " · Generating response",
        Some(LiveAnswerPhase::Finalizing) => " · Finalizing response",
        None => " · Working",
    }
}
