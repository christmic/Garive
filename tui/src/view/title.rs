use crate::application::{AppModel, ConnectionState, ExecutionState};

/// Builds the bounded, content-free title shown by the terminal emulator.
pub(crate) fn terminal_title(model: &AppModel) -> String {
    format!(
        "Garive · {} · {} · {}",
        session_label(model),
        connection_label(model.connection),
        execution_label(model.execution)
    )
}

fn session_label(model: &AppModel) -> String {
    let Some(selected) = model.selected_session.as_deref() else {
        return "Workspace".into();
    };
    model
        .sessions
        .iter()
        .position(|session| session.session_id == selected)
        .map(|index| format!("Session {}", index + 1))
        .unwrap_or_else(|| "Session active".into())
}

fn connection_label(state: ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Online => "Online",
        ConnectionState::Disconnected { .. } => "Disconnected",
        ConnectionState::Reconnecting { .. } => "Reconnecting",
        ConnectionState::Unavailable { .. } => "Unavailable",
    }
}

fn execution_label(state: ExecutionState) -> &'static str {
    match state {
        ExecutionState::Idle => "Ready",
        ExecutionState::Following => "Running",
        ExecutionState::Suspended => "Action required",
        ExecutionState::Failed => "Failed",
    }
}

#[cfg(test)]
mod tests {
    use garive_host_client::SessionSummary;

    use super::*;

    #[test]
    fn title_exposes_only_bounded_semantic_context() {
        let model = AppModel {
            sessions: vec![session("first"), session("private-session-canary")],
            selected_session: Some("private-session-canary".into()),
            connection: ConnectionState::Online,
            execution: ExecutionState::Suspended,
            notice: Some("private user and provider content".into()),
            ..AppModel::default()
        };

        let title = terminal_title(&model);

        assert_eq!(title, "Garive · Session 2 · Online · Action required");
        assert!(!title.contains("private"));
        assert!(!title.contains("canary"));
    }

    #[test]
    fn title_never_exposes_unknown_ids_or_safe_error_codes() {
        let model = AppModel {
            selected_session: Some("unloaded-private-id".into()),
            connection: ConnectionState::Unavailable {
                safe_code: "private-code-canary",
            },
            ..AppModel::default()
        };

        assert_eq!(
            terminal_title(&model),
            "Garive · Session active · Unavailable · Ready"
        );
    }

    fn session(id: &str) -> SessionSummary {
        SessionSummary {
            api_version: "v1".into(),
            session_id: id.into(),
            agent_instance_id: "private-agent-instance".into(),
            definition_id: "private-definition".into(),
            definition_revision: "private-revision".into(),
            opened_at: "2026-08-31T00:00:00Z".into(),
            latest_position: 1,
            latest_turn_id: Some("private-turn".into()),
            latest_turn_state: Some("completed".into()),
            turn_count: 1,
        }
    }
}
