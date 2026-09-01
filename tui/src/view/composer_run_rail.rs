//! Running-Turn state and control adjacent to the retained Composer draft.

use ratatui::text::{Line, Span};

use crate::application::{
    AppModel, CancelRequestPhase, ExecutionState, LiveAnswerAvailability, LiveAnswerPhase,
};

use super::{style::Palette, MotionFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TurnControlState {
    Running { cancel_available: bool },
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
            cancel_available: model.overlay.is_none(),
        }),
        None => None,
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

pub(super) fn visible(model: &AppModel) -> bool {
    project(model).is_some()
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

pub(super) fn line(
    model: &AppModel,
    colors: Palette,
    motion: MotionFrame,
) -> Option<Line<'static>> {
    let state = project(model)?;
    if !matches!(state, TurnControlState::Running { .. }) {
        let (label, tone) = match state {
            TurnControlState::CancelRequesting => ("Cancelling…", colors.warning),
            TurnControlState::CancelAwaitingTerminal => ("Stopping…", colors.notice),
            TurnControlState::CancelOutcomeUnknown => ("Cancel status unknown", colors.warning),
            TurnControlState::Running { .. } => unreachable!("running state handled above"),
        };
        let indicator = if state == TurnControlState::CancelOutcomeUnknown {
            "•"
        } else {
            motion.activity_indicator()
        };
        return Some(Line::from(vec![
            Span::styled(format!("{indicator} "), tone),
            Span::styled(label, colors.title.patch(tone)),
        ]));
    }
    let TurnControlState::Running { cancel_available } = state else {
        unreachable!("cancellation states handled above")
    };
    if transcript_owns_work(model) {
        return cancel_available.then(|| Line::styled("  esc to interrupt", colors.muted));
    }
    let label = running_label(model);
    if cancel_available {
        Some(Line::from(vec![
            Span::styled(format!("{} ", motion.activity_indicator()), colors.accent),
            Span::styled(label, colors.accent),
            Span::styled("  ·  esc to interrupt", colors.muted),
        ]))
    } else {
        Some(Line::from(vec![
            Span::styled(format!("{} ", motion.activity_indicator()), colors.accent),
            Span::styled(label, colors.accent),
        ]))
    }
}

fn transcript_owns_work(model: &AppModel) -> bool {
    if let Some(answer) = model.live_answer.current() {
        if answer.ended || answer.availability == LiveAnswerAvailability::Unavailable {
            return false;
        }
        if !answer.presented_text.is_empty() {
            return true;
        }
    }
    model.turn_blocks.iter().rev().any(|block| {
        model
            .selected_turn
            .as_deref()
            .is_none_or(|turn| block.key.turn_id == turn)
            && block
                .activities
                .iter()
                .any(|item| item.tone == crate::application::TimelineTone::Active)
    })
}

fn running_label(model: &AppModel) -> String {
    match model.live_answer.current() {
        Some(answer) if answer.availability == LiveAnswerAvailability::Unavailable => {
            "Live feedback unavailable".into()
        }
        Some(answer) if answer.ended => "Saving…".into(),
        Some(answer) => match answer.phase {
            Some(LiveAnswerPhase::Preparing) => "Preparing…".into(),
            Some(LiveAnswerPhase::Generating) => "Writing…".into(),
            Some(LiveAnswerPhase::Finalizing) => "Finishing…".into(),
            None => "Working…".into(),
        },
        None => "Working…".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{Overlay, TimelineItem, TimelineRole, TimelineTone},
        Theme,
    };

    #[test]
    fn running_rail_keeps_one_composer_adjacent_lifecycle_voice() {
        let colors = super::super::palette(Theme::Mono);
        let mut model = AppModel {
            execution: ExecutionState::Following,
            ..Default::default()
        };
        assert_eq!(
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            "• Working…  ·  esc to interrupt"
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
            "  esc to interrupt"
        );

        model.overlay = Some(Overlay::Help);
        assert!(line(&model, colors, MotionFrame::reduced()).is_none());
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
            "• Working…  ·  esc to interrupt"
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
            "• Cancelling…"
        );
        model.cancel_requests.mark_accepted("command");
        assert_eq!(
            line(&model, colors, MotionFrame::reduced())
                .unwrap()
                .to_string(),
            "• Stopping…"
        );
    }

    #[test]
    fn animated_run_rail_uses_the_shared_single_cell_pulse() {
        let colors = super::super::palette(Theme::Mono);
        let model = AppModel {
            execution: ExecutionState::Following,
            ..Default::default()
        };

        assert_eq!(
            line(&model, colors, MotionFrame::animated(0))
                .unwrap()
                .to_string(),
            "• Working…  ·  esc to interrupt"
        );
        assert_eq!(
            line(&model, colors, MotionFrame::animated(4))
                .unwrap()
                .to_string(),
            "◦ Working…  ·  esc to interrupt"
        );
    }
}
