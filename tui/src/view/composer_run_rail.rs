//! Running-Turn state and control adjacent to the retained Composer draft.

use ratatui::{buffer::Buffer, layout::Rect, text::Line, widgets::Widget};

use crate::application::{AppModel, ExecutionState, TimelineTone};

use super::{motion::status_motion, style::Palette, MotionFrame};

pub(super) fn render(
    model: &AppModel,
    colors: Palette,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
) {
    if area.is_empty() || model.execution != ExecutionState::Following {
        return;
    }
    line(model, colors, motion).render(area, buffer);
}

fn line(model: &AppModel, colors: Palette, motion: MotionFrame) -> Line<'static> {
    let mut line = Line::default();
    let show_work = !transcript_owns_work_indicator(model);
    let show_cancel = model.overlay.is_none();
    if show_work {
        line.push_span(ratatui::text::Span::styled(
            status_motion(model, motion).execution_label,
            colors.accent,
        ));
    }
    if show_work && show_cancel {
        line.push_span(ratatui::text::Span::styled(" · ", colors.muted));
    }
    if show_cancel {
        line.push_span(ratatui::text::Span::styled(" Esc ", colors.keycap));
        line.push_span(ratatui::text::Span::styled("cancel Turn", colors.muted));
    }
    line
}

fn transcript_owns_work_indicator(model: &AppModel) -> bool {
    model.live_answer.current().is_some()
        || model.turn_blocks.last().is_some_and(|turn| {
            turn.activities
                .iter()
                .any(|item| item.tone == TimelineTone::Active)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{Overlay, TimelineItem, TimelineRole},
        Theme,
    };

    #[test]
    fn running_rail_keeps_cancel_control_and_deduplicates_visible_work() {
        let colors = super::super::palette(Theme::Mono);
        let mut model = AppModel {
            execution: ExecutionState::Following,
            ..Default::default()
        };
        assert_eq!(
            line(&model, colors, MotionFrame::reduced()).to_string(),
            "Agent running ·  Esc cancel Turn"
        );

        model.push_test_timeline_item(TimelineItem {
            stable_key: "user".into(),
            position: 1,
            role: TimelineRole::User,
            tone: TimelineTone::Neutral,
            text: "request".into(),
        });
        model.push_test_timeline_item(TimelineItem {
            stable_key: "activity".into(),
            position: 2,
            role: TimelineRole::Status,
            tone: TimelineTone::Active,
            text: "Reading file".into(),
        });
        assert_eq!(
            line(&model, colors, MotionFrame::reduced()).to_string(),
            " Esc cancel Turn"
        );

        model.overlay = Some(Overlay::Help);
        assert!(line(&model, colors, MotionFrame::reduced())
            .spans
            .is_empty());
        model.turn_blocks.clear();
        assert_eq!(
            line(&model, colors, MotionFrame::reduced()).to_string(),
            "Agent running"
        );
        model.overlay = None;
        assert_eq!(
            line(&model, colors, MotionFrame::reduced()).to_string(),
            "Agent running ·  Esc cancel Turn"
        );
    }
}
