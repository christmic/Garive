use garive_host_client::{
    CreateSessionResponse, HostClientError, HostClientErrorCode, LiveHostClient,
    TurnCommandResponse,
};
use serde_json::json;

use super::*;
use crate::{
    parse_launch_config,
    persistence::{now, PendingCommand, PendingKind, Preferences, StateStore},
};

use super::super::{RestoredState, LIMITS};

#[test]
fn mutation_responses_require_the_exact_pending_owner() {
    let start = pending(PendingKind::StartTurn, "start", Some("session-a"), None);
    assert_eq!(
        matches_turn_accepted(
            std::slice::from_ref(&start),
            "start",
            "session-a",
            "hello",
            "session-a",
            "turn-new",
        ),
        Some(PendingKind::StartTurn)
    );
    assert_eq!(
        matches_turn_accepted(
            &[start],
            "start",
            "session-b",
            "hello",
            "session-b",
            "turn-new",
        ),
        None
    );

    let cancel = pending(
        PendingKind::CancelTurn,
        "cancel",
        Some("session-a"),
        Some("turn-a"),
    );
    assert_eq!(
        matches_turn_accepted(
            std::slice::from_ref(&cancel),
            "cancel",
            "session-a",
            "",
            "session-a",
            "turn-a",
        ),
        Some(PendingKind::CancelTurn)
    );
    assert_eq!(
        matches_turn_accepted(
            &[cancel],
            "cancel",
            "session-a",
            "",
            "session-a",
            "turn-other",
        ),
        None
    );

    let continuation = pending(
        PendingKind::ContinueTurn,
        "continue",
        Some("session-a"),
        Some("turn-a"),
    );
    assert_eq!(
        matches_turn_accepted(
            std::slice::from_ref(&continuation),
            "continue",
            "session-a",
            r#"{"approved":true}"#,
            "session-a",
            "turn-a",
        ),
        Some(PendingKind::ContinueTurn)
    );
    assert_eq!(
        matches_turn_accepted(
            &[continuation],
            "continue",
            "session-a",
            r#"{"approved":false}"#,
            "session-a",
            "turn-a",
        ),
        None
    );

    let create = pending(PendingKind::CreateSession, "create", None, None);
    assert!(matches_session_created(
        std::slice::from_ref(&create),
        "create"
    ));
    assert!(!matches_session_created(
        &[create.clone(), create],
        "create"
    ));
}

#[tokio::test]
async fn unknown_turn_response_preserves_pending_queue_and_execution() {
    let owner = pending(
        PendingKind::ContinueTurn,
        "expected",
        Some("session-a"),
        Some("turn-a"),
    );
    let mut state = runtime(vec![owner.clone()]);
    state.model.selected_session = Some("session-a".into());
    state.model.selected_turn = Some("turn-a".into());
    state.model.active_execution_id = Some("execution-a".into());
    state.model.execution = ExecutionState::Suspended;
    state.model.connection = ConnectionState::Online;
    state.queued_prompt = Some("queued private prompt".into());

    handle_host(
        HostMessage::TurnAccepted {
            command_id: "unknown".into(),
            session_id: "session-a".into(),
            submitted_text: r#"{"approved":true}"#.into(),
            response: turn_response("session-a", "turn-a", "execution-new"),
        },
        &mut state,
    );

    assert_eq!(state.pending, vec![owner]);
    assert_eq!(
        state.queued_prompt.as_deref(),
        Some("queued private prompt")
    );
    assert_eq!(state.model.selected_turn.as_deref(), Some("turn-a"));
    assert_eq!(
        state.model.active_execution_id.as_deref(),
        Some("execution-a")
    );
    assert_eq!(state.model.execution, ExecutionState::Suspended);
}

#[tokio::test]
async fn duplicate_session_response_does_not_consume_a_queued_prompt() {
    let mut state = runtime(Vec::new());
    state.model.execution = ExecutionState::Idle;
    state.model.connection = ConnectionState::Online;
    state.queued_prompt = Some("keep queued".into());

    handle_host(
        HostMessage::SessionCreated {
            command_id: "already-finished".into(),
            response: CreateSessionResponse {
                session_id: "session-new".into(),
                agent_instance_id: "agent-new".into(),
                committed_position: 1,
            },
        },
        &mut state,
    );

    assert_eq!(state.queued_prompt.as_deref(), Some("keep queued"));
    assert_eq!(state.model.selected_session, None);
    assert_eq!(state.model.execution, ExecutionState::Idle);
}

#[tokio::test]
async fn unknown_mutation_failure_does_not_reject_an_unrelated_owner() {
    let owner = pending(PendingKind::StartTurn, "start", Some("session-a"), None);
    let mut state = runtime(vec![owner.clone()]);
    state.model.selected_session = Some("session-a".into());
    state.model.execution = ExecutionState::Following;
    state.model.connection = ConnectionState::Online;
    state.model.overlay = None;

    handle_host(
        HostMessage::Failed {
            operation: HostOperation::Mutation {
                command_id: "unknown".into(),
            },
            error: HostClientError {
                code: HostClientErrorCode::HostFailure,
                status: Some(500),
            },
        },
        &mut state,
    );

    assert_eq!(state.pending, vec![owner]);
    assert_eq!(state.model.execution, ExecutionState::Following);
    assert_eq!(state.model.overlay, None);
}

#[tokio::test]
async fn exact_start_response_consumes_only_its_owner() {
    let owner = pending(PendingKind::StartTurn, "start", Some("session-a"), None);
    let other = pending(PendingKind::StartTurn, "other", Some("session-b"), None);
    let mut state = runtime(vec![owner, other.clone()]);
    state.model.selected_session = Some("session-a".into());
    let _ = state.model.composer.replace("hello");

    handle_host(
        HostMessage::TurnAccepted {
            command_id: "start".into(),
            session_id: "session-a".into(),
            submitted_text: "hello".into(),
            response: turn_response("session-a", "turn-new", "execution-new"),
        },
        &mut state,
    );

    assert_eq!(state.pending, vec![other]);
    assert!(state.model.composer.text().is_empty());
    assert_eq!(state.model.selected_turn.as_deref(), Some("turn-new"));
    assert_eq!(
        state.model.active_execution_id.as_deref(),
        Some("execution-new")
    );
    assert_eq!(state.model.execution, ExecutionState::Following);
}

#[tokio::test]
async fn replay_claim_is_single_owner_until_the_mutation_result() {
    let owner = pending(PendingKind::StartTurn, "retry", Some("session-a"), None);
    let mut state = runtime(vec![owner.clone()]);
    state.model.selected_session = Some("session-a".into());
    assert!(state.begin_exact_retry(&owner.command_id));

    assert!(replay_queued_for_session(&mut state, "session-a"));
    assert!(state.exact_retry_was_replayed());
    assert!(state.exact_retry_in_progress());

    assert!(!replay_queued_for_session(&mut state, "session-a"));
    assert!(state.exact_retry_was_replayed());
    assert_eq!(state.pending, vec![owner]);

    handle_host(
        HostMessage::TurnAccepted {
            command_id: "retry".into(),
            session_id: "session-a".into(),
            submitted_text: "hello".into(),
            response: turn_response("session-a", "turn-new", "execution-new"),
        },
        &mut state,
    );

    assert!(!state.exact_retry_in_progress());
    assert!(state.pending.is_empty());
}

fn runtime(pending: Vec<PendingCommand>) -> RuntimeState {
    let config = parse_launch_config(["garive-tui", "--host", "http://127.0.0.1:1", "--ephemeral"])
        .expect("test launch config is valid");
    let client = LiveHostClient::new(&config.host, LIMITS).expect("test Host URL is valid");
    let store = StateStore::open(None, true).expect("ephemeral state is available");
    let preferences = Preferences::default();
    let (sender, _receiver) = tokio::sync::mpsc::channel(16);
    let (action_sender, _action_receiver) = tokio::sync::mpsc::channel(16);
    RuntimeState::new(
        config,
        client,
        sender,
        action_sender,
        crate::runtime::TerminalTheme::default(),
        RestoredState {
            store,
            preferences: preferences.clone(),
            pending,
            pending_quarantined: 0,
            history: Vec::new(),
            history_error: false,
        },
    )
}

fn pending(
    kind: PendingKind,
    command_id: &str,
    session_id: Option<&str>,
    turn_id: Option<&str>,
) -> PendingCommand {
    let (suspension_id, expected_session_version, requested_through_position, request_payload) =
        match kind {
            PendingKind::CreateSession => (
                None,
                None,
                None,
                json!({"agent_definition_id":"definition-a"}),
            ),
            PendingKind::StartTurn => (None, None, None, json!({"text":"hello"})),
            PendingKind::CancelTurn => (
                None,
                None,
                Some(7),
                json!({
                    "session_id": session_id,
                    "requested_through_position": 7
                }),
            ),
            PendingKind::ContinueTurn => (
                Some("suspension-a".into()),
                Some(3),
                None,
                json!({"input_json":{"approved":true}}),
            ),
        };
    PendingCommand {
        schema_version: 1,
        command_id: command_id.into(),
        kind,
        session_id: session_id.map(str::to_owned),
        turn_id: turn_id.map(str::to_owned),
        suspension_id,
        expected_session_version,
        requested_through_position,
        request_payload,
        request_digest: String::new(),
        created_at: now(),
    }
    .seal()
    .expect("test pending command is valid")
}

fn turn_response(session_id: &str, turn_id: &str, execution_id: &str) -> TurnCommandResponse {
    TurnCommandResponse {
        session_id: session_id.into(),
        turn_id: turn_id.into(),
        execution_id: execution_id.into(),
        committed_position: 8,
    }
}
