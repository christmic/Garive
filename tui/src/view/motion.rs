use crate::application::{AppModel, ConnectionState, ExecutionState};

const PULSE: [&str; 4] = ["·", "•", "●", "•"];

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

    fn pulse(self) -> Option<&'static str> {
        (!self.reduced).then(|| PULSE[(self.tick as usize / 2) % PULSE.len()])
    }
}

pub(crate) struct StatusMotion {
    pub(crate) connection_icon: &'static str,
    pub(crate) execution_label: String,
}

pub(crate) fn status_motion_active(model: &AppModel) -> bool {
    matches!(
        model.connection,
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. }
    ) || model.execution == ExecutionState::Following
}

pub(crate) fn status_motion_enabled(model: &AppModel, reduced: bool) -> bool {
    !reduced && status_motion_active(model)
}

pub(crate) fn status_motion(model: &AppModel, frame: MotionFrame) -> StatusMotion {
    let connection_icon = if matches!(
        model.connection,
        ConnectionState::Connecting | ConnectionState::Reconnecting { .. }
    ) {
        frame.pulse().unwrap_or("○")
    } else if model.connection == ConnectionState::Online {
        "●"
    } else {
        "○"
    };
    let execution = match model.execution {
        ExecutionState::Idle => "ready",
        ExecutionState::Following => "running",
        ExecutionState::Suspended => "action required",
        ExecutionState::Failed => "failed",
    };
    let execution_label = match (model.execution, frame.pulse()) {
        (ExecutionState::Following, Some(pulse)) => format!("{pulse} {execution}"),
        _ => execution.into(),
    };
    StatusMotion {
        connection_icon,
        execution_label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_statuses_animate_but_reduced_motion_is_stable() {
        let mut model = AppModel {
            connection: ConnectionState::Connecting,
            execution: ExecutionState::Following,
            ..Default::default()
        };
        assert!(status_motion_active(&model));
        assert!(status_motion_enabled(&model, false));
        assert!(!status_motion_enabled(&model, true));
        assert_eq!(
            status_motion(&model, MotionFrame::animated(0)).connection_icon,
            "·"
        );
        assert_eq!(
            status_motion(&model, MotionFrame::animated(4)).execution_label,
            "● running"
        );
        let reduced = status_motion(&model, MotionFrame::reduced());
        assert_eq!(reduced.connection_icon, "○");
        assert_eq!(reduced.execution_label, "running");

        model.connection = ConnectionState::Online;
        model.execution = ExecutionState::Idle;
        assert!(!status_motion_active(&model));
        assert!(!status_motion_enabled(&model, false));
    }
}
