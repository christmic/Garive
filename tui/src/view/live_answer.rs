use ratatui::text::{Line, Span};

use crate::{
    application::{LiveAnswer, LiveAnswerAvailability, LiveAnswerPhase},
    Theme,
};

use super::{markdown::render_markdown, palette};

pub(super) fn render(
    answer: &LiveAnswer,
    theme: Theme,
    width: u16,
    reduced_motion: bool,
) -> Vec<Line<'static>> {
    let colors = palette(theme);
    let mut lines = vec![Line::from(vec![
        Span::styled("◆  GARIVE", colors.agent),
        Span::styled(phase_copy(answer.phase), colors.muted),
    ])];
    match answer.availability {
        LiveAnswerAvailability::Unavailable => lines.push(Line::styled(
            "   Live feedback unavailable · waiting for saved result",
            colors.muted,
        )),
        LiveAnswerAvailability::Available => {
            if answer.presented_text.is_empty() {
                lines.push(Line::styled("   ", colors.normal));
            } else {
                lines.extend(render_markdown(
                    &answer.presented_text,
                    "   ",
                    colors.normal,
                    colors.agent,
                    colors.muted,
                    super::markdown_syntax::SyntaxPalette::from_palette(colors),
                    width,
                ));
            }
            if answer.caret_visible() && !reduced_motion {
                if let Some(line) = lines.last_mut() {
                    line.spans.push(Span::styled("▍", colors.accent));
                }
            }
        }
    }
    lines.push(Line::default());
    lines
}

fn phase_copy(phase: Option<LiveAnswerPhase>) -> &'static str {
    match phase {
        Some(LiveAnswerPhase::Preparing) => " · Preparing context",
        Some(LiveAnswerPhase::Generating) => " · Generating response",
        Some(LiveAnswerPhase::Finalizing) => " · Finalizing response",
        None => " · Working",
    }
}
