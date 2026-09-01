use crate::application::{AppModel, ExecutionState};

/// Pure presentation input for time-varying status components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MotionFrame {
    tick: u64,
    reduced: bool,
}

impl MotionFrame {
    pub(crate) const fn animated(tick: u64) -> Self {
        Self {
            tick,
            reduced: false,
        }
    }

    pub(crate) const fn reduced() -> Self {
        Self {
            tick: 0,
            reduced: true,
        }
    }

    pub(crate) const fn is_reduced(self) -> bool {
        self.reduced
    }

    /// Returns the shared one-cell activity pulse used by transient work rows.
    pub(crate) const fn activity_indicator(self) -> &'static str {
        if self.reduced || (self.tick / 4).is_multiple_of(2) {
            "•"
        } else {
            "◦"
        }
    }
}

pub(crate) fn status_motion_active(model: &AppModel) -> bool {
    model.execution == ExecutionState::Following
}

pub(crate) fn status_motion_enabled(model: &AppModel, reduced: bool) -> bool {
    !reduced && status_motion_active(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_output_requests_ticks_unless_motion_is_reduced() {
        let mut model = AppModel {
            execution: ExecutionState::Following,
            ..Default::default()
        };
        assert!(status_motion_active(&model));
        assert!(status_motion_enabled(&model, false));
        assert!(!status_motion_enabled(&model, true));
        model.execution = ExecutionState::Idle;
        assert!(!status_motion_active(&model));
        assert!(!status_motion_enabled(&model, false));
    }

    #[test]
    fn activity_indicator_pulses_without_changing_cell_width() {
        assert_eq!(MotionFrame::animated(0).activity_indicator(), "•");
        assert_eq!(MotionFrame::animated(3).activity_indicator(), "•");
        assert_eq!(MotionFrame::animated(4).activity_indicator(), "◦");
        assert_eq!(MotionFrame::animated(7).activity_indicator(), "◦");
        assert_eq!(MotionFrame::animated(8).activity_indicator(), "•");
        assert_eq!(MotionFrame::reduced().activity_indicator(), "•");
        assert_eq!(
            unicode_width::UnicodeWidthStr::width(MotionFrame::animated(4).activity_indicator()),
            1
        );
    }
}
