//! Running-Turn state and control adjacent to the retained Composer draft.

use ratatui::{buffer::Buffer, layout::Rect, text::Line, widgets::Widget};

use crate::application::{AppModel, CancelRequestPhase, ExecutionState, TimelineTone};

use super::{motion::status_motion, style::Palette, MotionFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TurnControlState {
    Running {
        transcript_owns_work: bool,
        cancel_available: bool,
    },
    CancelRequesting,
    CancelAwaitingTerminal,
    CancelOutcomeUnknown,
}

fn project(model: &AppModel) -> Option<TurnControlState> {
    match model.selected_cancel_request().map(|request| request.phase) {
        Some(CancelRequestPhase::Requesting) => Some(TurnControlState::CancelRequesting),
        Some(CancelRequestPhase::AwaitingTerminal) => {
            Some(TurnControlState::CancelAwaitingTerminal)
        }
        Some(CancelRequestPhase::OutcomeUnknown) => Some(TurnControlState::CancelOutcomeUnknown),
        None if model.execution == ExecutionState::Following => Some(TurnControlState::Running {
            transcript_owns_work: transcript_owns_work_indicator(model),
            cancel_available: model.overlay.is_none(),
        }),
        None => None,
    }
}

pub(super) fn render(
    model: &AppModel,
    colors: Palette,
    motion: MotionFrame,
    area: Rect,
    buffer: &mut Buffer,
) {
    if area.is_empty() {
        return;
    }
    if let Some(line) = line(model, colors, motion) {
        line.render(area, buffer);
    }
}

pub(super) fn has_cancel_request(model: &AppModel) -> bool {
    matches!(
        project(model),
        Some(
            TurnControlState::CancelRequesting
                | TurnControlState::CancelAwaitingTerminal
                | TurnControlState::CancelOutcomeUnknown
        )
    )
}

pub(super) fn linear_status(model: &AppModel) -> Option<&'static str> {
    match project(model) {
        Some(TurnControlState::CancelRequesting) => {
            Some("Cancellation requested. Draft retained. Waiting for Host acceptance.")
        }
        Some(TurnControlState::CancelAwaitingTerminal) => {
            Some("Cancellation accepted. Draft retained. Waiting for durable Turn termination.")
        }
        Some(TurnControlState::CancelOutcomeUnknown) => {
            Some("Cancellation outcome unknown. Draft retained. The recovery overlay owns input.")
        }
        Some(TurnControlState::Running { .. }) | None => None,
    }
}

pub(super) fn minimum_status(model: &AppModel) -> Option<&'static str> {
    match project(model) {
        Some(TurnControlState::CancelRequesting) => Some("Cancelling…"),
        Some(TurnControlState::CancelAwaitingTerminal) => Some("Stopping…"),
        Some(TurnControlState::CancelOutcomeUnknown) => Some("Cancel status unknown"),
        Some(TurnControlState::Running {
            cancel_available: false,
            ..
        }) => Some("Run continues · overlay owns input"),
        Some(TurnControlState::Running {
            cancel_available: true,
            ..
        }) => Some("Run continues · Esc cancel"),
        None => None,
    }
}

fn line(model: &AppModel, colors: Palette, motion: MotionFrame) -> Option<Line<'static>> {
    let state = project(model)?;
    if !matches!(state, TurnControlState::Running { .. }) {
        let (marker, label, tone) = match state {
            TurnControlState::CancelRequesting => (" … ", "Cancelling…", colors.warning),
            TurnControlState::CancelAwaitingTerminal => (" … ", "Stopping…", colors.notice),
            TurnControlState::CancelOutcomeUnknown => {
                (" ! ", "Cancel status unknown", colors.warning)
            }
            TurnControlState::Running { .. } => unreachable!("running state handled above"),
        };
        return Some(Line::from(vec![
            ratatui::text::Span::styled(marker, tone),
            ratatui::text::Span::styled(label, colors.title),
        ]));
    }
    let TurnControlState::Running {
        transcript_owns_work,
        cancel_available,
    } = state
    else {
        unreachable!("cancellation states handled above")
    };
    let mut line = Line::default();
    let show_work = !transcript_owns_work;
    let show_cancel = cancel_available;
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
        line.push_span(ratatui::text::Span::styled("interrupt", colors.muted));
    }
    Some(line)
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
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            "• Working… ·  Esc interrupt"
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
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            " Esc interrupt"
        );

        model.overlay = Some(Overlay::Help);
        assert!(line(&model, colors, MotionFrame::reduced())
            .unwrap()
            .spans
            .is_empty());
        model.turn_blocks.clear();
        assert_eq!(
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            "• Working…"
        );
        model.overlay = None;
        assert_eq!(
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            "• Working… ·  Esc interrupt"
        );
    }

    #[test]
    fn cancellation_phases_replace_generic_running_and_frozen_copy() {
        let colors = super::super::palette(Theme::Mono);
        let mut model = AppModel {
            execution: ExecutionState::Following,
            selected_session: Some("session".into()),
            selected_turn: Some("turn".into()),
            composer_is_frozen: true,
            ..Default::default()
        };
        model
            .cancel_requests
            .begin("command".into(), "session".into(), "turn".into());
        assert_eq!(
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            " … Cancelling…"
        );
        model.cancel_requests.mark_accepted("command");
        assert_eq!(
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            " … Stopping…"
        );
    }
}
