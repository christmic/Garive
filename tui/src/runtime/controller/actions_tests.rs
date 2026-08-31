use serde_json::json;

use super::*;
use crate::persistence::{now, PendingKind};

#[tokio::test]
async fn retry_rejects_a_command_that_is_still_in_flight() {
    let pending = create_pending("in-flight");
    let mut state = RuntimeState::test_ephemeral(Vec::new());
    state.ephemeral_confirmed = true;
    assert!(state.admit_pending(pending.clone()));

    retry_pending(&mut state);

    assert_eq!(state.pending, vec![pending]);
    assert!(!state.exact_retry_in_progress());
    assert_eq!(
        state.model.notice.as_deref(),
        Some("The pending command is still in flight; exact retry is unavailable.")
    );
}

#[tokio::test]
async fn repeated_retry_keeps_the_first_recovery_owner() {
    let pending = create_pending("unknown");
    let mut state = RuntimeState::test_ephemeral(vec![pending.clone()]);

    retry_pending(&mut state);
    assert!(state.exact_retry_in_progress());

    retry_pending(&mut state);

    assert_eq!(state.pending, vec![pending]);
    assert!(state.exact_retry_in_progress());
    assert_eq!(
        state.model.notice.as_deref(),
        Some("An exact retry is already in progress.")
    );
}

fn create_pending(command_id: &str) -> PendingCommand {
    PendingCommand {
        schema_version: 1,
        command_id: command_id.into(),
        kind: PendingKind::CreateSession,
        session_id: None,
        turn_id: None,
        suspension_id: None,
        expected_session_version: None,
        requested_through_position: None,
        request_payload: json!({"agent_definition_id":"definition-a"}),
        request_digest: String::new(),
        created_at: now(),
    }
    .seal()
    .expect("test pending command is valid")
}
