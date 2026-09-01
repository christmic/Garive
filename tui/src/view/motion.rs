use crate::application::{AppModel, ExecutionState};

/// Pure presentation input for time-varying status components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MotionFrame {
    _tick: u64,
    reduced: bool,
}

impl MotionFrame {
    pub(crate) const fn animated(tick: u64) -> Self {
        Self {
            _tick: tick,
            reduced: false,
        }
    }

    pub(crate) const fn reduced() -> Self {
        Self {
            _tick: 0,
            reduced: true,
        }
    }

    pub(crate) const fn is_reduced(self) -> bool {
        self.reduced
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
}
