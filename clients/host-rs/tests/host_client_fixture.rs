use std::sync::Arc;

use garive_host_client::{
    reduce_host_events, ApprovalDecision, ClientLimits, HostClientErrorCode, HostEvent,
    HostTerminal, HostView, LiveHostClient, LiveOutputEventKind, HOST_CLIENT_FAILURES,
};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

#[derive(Deserialize)]
struct Fixture {
    limits: FixtureLimits,
    session_id: String,
    valid_stream: Vec<HostEvent>,
    expected: Expected,
    reconnect: Reconnect,
    disconnect_before_terminal: Vec<HostEvent>,
    invalid_streams: Vec<InvalidStream>,
    host_errors: Vec<HostError>,
    typed_continuation: TypedContinuation,
    failure_codes: Vec<String>,
}

#[derive(Deserialize)]
struct FixtureLimits {
    max_command_bytes: usize,
    max_event_bytes: usize,
    max_events: usize,
}

#[derive(Deserialize)]
struct Expected {
    cursor: u64,
    terminal: String,
    text: String,
    unknown_events: Vec<String>,
}

#[derive(Deserialize)]
struct Reconnect {
    after_position: u64,
    events: Vec<HostEvent>,
    expected_applied_positions: Vec<u64>,
}

#[derive(Deserialize)]
struct InvalidStream {
    mutation: String,
    expected: String,
}

#[derive(Deserialize)]
struct HostError {
    status: u16,
    code: String,
}

#[derive(Deserialize)]
struct TypedContinuation {
    canonical_json: String,
    non_canonical_json: String,
    non_canonical_error: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/host/live-host-client-v1.json"
    ))
    .expect("fixture must decode")
}

fn limits(fixture: &Fixture) -> ClientLimits {
    ClientLimits {
        max_command_bytes: fixture.limits.max_command_bytes,
        max_event_bytes: fixture.limits.max_event_bytes,
        max_events: fixture.limits.max_events,
        follow_deadline_ms: 2_000,
    }
}

#[test]
fn shared_fixture_reduces_gaps_unknown_events_and_reconnects() {
    let fixture = fixture();
    let view = reduce_host_events(
        &fixture.session_id,
        &fixture.valid_stream,
        HostView::default(),
        fixture.limits.max_events,
    )
    .expect("valid stream");
    assert_eq!(view.cursor, fixture.expected.cursor);
    assert_eq!(view.terminal, Some(HostTerminal::Completed));
    assert_eq!(fixture.expected.terminal, "completed");
    assert_eq!(view.text, fixture.expected.text);
    assert_eq!(view.unknown_events, fixture.expected.unknown_events);

    let reconnect = reduce_host_events(
        &fixture.session_id,
        &fixture.reconnect.events,
        HostView::at_cursor(fixture.reconnect.after_position),
        fixture.limits.max_events,
    )
    .expect("reconnect stream");
    assert_eq!(
        fixture.reconnect.expected_applied_positions,
        vec![5, reconnect.cursor]
    );

    let disconnected = reduce_host_events(
        &fixture.session_id,
        &fixture.disconnect_before_terminal,
        HostView::default(),
        fixture.limits.max_events,
    )
    .expect("non-terminal prefix remains valid");
    assert_eq!(disconnected.terminal, None);
}

#[test]
fn h3_activity_updates_only_from_greater_committed_positions() {
    let events: Vec<HostEvent> = serde_json::from_value(serde_json::json!([
        {"api_version":"v1","session_id":"session-1","position":2,"event":"agent.activity.prepared","turn_id":"turn-1","execution_id":"execution-1","text":"","activity":{"api_version":"v1","activity_id":"activity-1","kind":"tool","label_key":"agent.activity.read_file","state":"prepared","source_position":2,"terminal":false}},
        {"api_version":"v1","session_id":"session-1","position":3,"event":"agent.activity.started","turn_id":"turn-1","execution_id":"execution-1","text":"","activity":{"api_version":"v1","activity_id":"activity-1","kind":"tool","label_key":"agent.activity.read_file","state":"running","source_position":3,"terminal":false}},
        {"api_version":"v1","session_id":"session-1","position":4,"event":"agent.activity.completed","turn_id":"turn-1","execution_id":"execution-1","text":"","activity":{"api_version":"v1","activity_id":"activity-1","kind":"tool","label_key":"agent.activity.read_file","state":"completed","source_position":4,"terminal":true}}
    ])).unwrap();
    let view = reduce_host_events("session-1", &events, HostView::default(), 8).unwrap();
    assert_eq!(view.activities["activity-1"].state, "completed");

    let mut invalid = events;
    invalid[2].activity.as_mut().unwrap().terminal = false;
    assert_eq!(
        reduce_host_events("session-1", &invalid, HostView::default(), 8)
            .unwrap_err()
            .code,
        HostClientErrorCode::InvalidEvent
    );
}

#[test]
fn shared_fixture_mutations_have_exact_failures() {
    let fixture = fixture();
    for case in &fixture.invalid_streams {
        let mut events = fixture.valid_stream.clone();
        let max_events = match case.mutation.as_str() {
            "api_version_v2" => {
                events[0].api_version = "v2".into();
                fixture.limits.max_events
            }
            "session_other" => {
                events[0].session_id = "other".into();
                fixture.limits.max_events
            }
            "position_zero" => {
                events[0].position = 0;
                fixture.limits.max_events
            }
            "position_backward" => {
                events[2].position = 1;
                fixture.limits.max_events
            }
            "duplicate_conflict" => {
                let mut duplicate = events[0].clone();
                duplicate.text = "conflict".into();
                events.insert(1, duplicate);
                fixture.limits.max_events
            }
            "event_count_17" => {
                events = vec![events[0].clone(); 17];
                fixture.limits.max_events
            }
            mutation => panic!("unknown mutation {mutation}"),
        };
        let error = reduce_host_events(
            &fixture.session_id,
            &events,
            HostView::default(),
            max_events,
        )
        .expect_err("mutation must fail");
        assert_eq!(error.code.wire_name(), case.expected);
    }
    assert_eq!(
        HOST_CLIENT_FAILURES
            .iter()
            .map(|code| code.wire_name())
            .collect::<Vec<_>>(),
        fixture.failure_codes
    );
}

#[tokio::test]
async fn live_client_round_trips_real_http_and_sse() {
    let fixture = fixture();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let turn_stream: Vec<HostEvent> = fixture
        .valid_stream
        .iter()
        .filter(|event| !event.turn_id.is_empty())
        .cloned()
        .collect();
    let responses = vec![
        http_json(
            201,
            r#"{"session_id":"session-client","agent_instance_id":"agent-1","committed_position":1}"#,
        ),
        http_json(
            202,
            r#"{"api_version":"v1","session_id":"session-client","delivery":"direct","turns":[{"agent_id":"definition-1","turn_id":"turn-client","execution_id":"execution-client","committed_position":2}]}"#,
        ),
        http_sse(&turn_stream),
    ];
    let (base_url, server) = serve(responses, Arc::clone(&bodies)).await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    let session = client
        .create_session("create-stable-1", "definition-1")
        .await
        .expect("create");
    let turn = client
        .start_turn_direct(
            "turn-stable-1",
            &session.session_id,
            "definition-1",
            "private command",
        )
        .await
        .expect("start");
    let view = client
        .follow_until_terminal(&session.session_id, &turn.turns[0].turn_id, 0)
        .await
        .expect("follow");
    server.await.expect("server task");

    assert_eq!(turn.turns[0].execution_id, "execution-client");
    assert_eq!(view.terminal, Some(HostTerminal::Completed));
    assert_eq!(view.text, "durable answer");
    let requests = bodies.lock().await;
    assert!(requests[0].starts_with("POST /v1/sessions HTTP/1.1\r\n"));
    assert!(requests[0].contains("idempotency-key: create-stable-1"));
    assert!(requests[1].starts_with("POST /v1/sessions/session-client/turns HTTP/1.1\r\n"));
    assert!(requests[2].starts_with(
        "GET /v1/sessions/session-client/turns/turn-client/events?after_position=0 HTTP/1.1\r\n"
    ));
}

#[tokio::test]
async fn live_snapshot_uses_h4_bounds_instead_of_h1_event_bound() {
    let fixture = fixture();
    let text = "\0".repeat(1_024 * 1_024);
    let body = serde_json::json!({
        "api_version": "v1",
        "session_id": "session-live-bounds",
        "turn_id": "turn-live-bounds",
        "execution_id": "execution-live-bounds",
        "stream_id": "12345678-1234-4234-8234-123456789abc",
        "sequence": 7,
        "kind": "snapshot",
        "text": text,
        "through_sequence": 7
    });
    let encoded = serde_json::to_string(&body).unwrap();
    assert!(encoded.len() > fixture.limits.max_event_bytes);
    assert!(encoded.len() <= 6 * 1_024 * 1_024 + 2_048);
    assert_eq!(text.len(), 1_024 * 1_024);

    let response = http_live_sse(&encoded);
    let (base_url, server) = serve(vec![response], Arc::new(Mutex::new(Vec::new()))).await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let follow = tokio::spawn(async move {
        client
            .follow_live_output("session-live-bounds", sender)
            .await
    });

    let snapshot = receiver.recv().await.expect("snapshot");
    assert!(matches!(
        snapshot.kind,
        LiveOutputEventKind::Snapshot {
            text: received,
            through_sequence: 7
        } if received == text
    ));
    assert_eq!(
        follow.await.unwrap().unwrap_err().code,
        HostClientErrorCode::TransportFailure
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn goal_page_is_typed_ordered_and_graph_checked() {
    let fixture = fixture();
    let valid = serde_json::json!({
        "api_version": "v1",
        "session_id": "session-goals",
        "goals": [
            {"api_version":"v1","goal_id":"child","revision":2,"state":"active","definition_digest":"a".repeat(64),"objective":"child","objective_truncated":false,"parent_goal_id":"root","attempt_number":1,"criteria_total":1,"criteria_satisfied":0},
            {"api_version":"v1","goal_id":"root","revision":1,"state":"draft","definition_digest":"b".repeat(64),"objective":"root","objective_truncated":false,"attempt_number":0,"criteria_total":1,"criteria_satisfied":0}
        ],
        "session_version": 3,
        "observed_max_position": 3
    });
    let cyclic = serde_json::json!({
        "api_version": "v1",
        "session_id": "session-goals",
        "goals": [
            {"api_version":"v1","goal_id":"a","revision":1,"state":"draft","definition_digest":"a".repeat(64),"objective":"a","objective_truncated":false,"parent_goal_id":"b","attempt_number":0,"criteria_total":1,"criteria_satisfied":0},
            {"api_version":"v1","goal_id":"b","revision":1,"state":"draft","definition_digest":"b".repeat(64),"objective":"b","objective_truncated":false,"parent_goal_id":"a","attempt_number":0,"criteria_total":1,"criteria_satisfied":0}
        ],
        "session_version": 3,
        "observed_max_position": 3
    });
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, &valid.to_string()),
            http_json(200, &cyclic.to_string()),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");

    let page = client.get_goals("session-goals").await.expect("goals");
    assert_eq!(page.goals[0].parent_goal_id.as_deref(), Some("root"));
    assert_eq!(
        client.get_goals("session-goals").await.unwrap_err().code,
        HostClientErrorCode::InvalidEvent
    );
    server.await.expect("server task");
    assert!(
        requests.lock().await[0].starts_with("GET /v1/sessions/session-goals/goals HTTP/1.1\r\n")
    );
}

#[tokio::test]
async fn goal_create_is_canonical_identity_bound_and_strictly_validated() {
    let fixture = fixture();
    let definition = goal_definition_json("goal-client", "deliver", "session-goal-create");
    let valid = serde_json::json!({
        "api_version":"v1","session_id":"session-goal-create","goal_id":"goal-client",
        "revision":1,"state":"draft","session_version":2,"committed_position":2
    });
    let invalid = serde_json::json!({
        "api_version":"v1","session_id":"session-goal-create","goal_id":"other",
        "revision":1,"state":"draft","session_version":2,"committed_position":2
    });
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, &valid.to_string()),
            http_json(200, &invalid.to_string()),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).unwrap();
    let response = client
        .create_goal("create-goal-client", "session-goal-create", 1, &definition)
        .await
        .unwrap();
    assert_eq!(response.goal_id, "goal-client");
    assert_eq!(
        client
            .create_goal(
                "create-goal-client-2",
                "session-goal-create",
                1,
                &definition,
            )
            .await
            .unwrap_err()
            .code,
        HostClientErrorCode::InvalidEvent
    );
    server.await.unwrap();
    let request = &requests.lock().await[0];
    assert!(request.starts_with("POST /v1/sessions/session-goal-create/goals HTTP/1.1\r\n"));
    assert!(request.contains("idempotency-key: create-goal-client"));
    assert!(request.contains("\"expected_session_version\":1"));
}

#[tokio::test]
async fn goal_cancel_binds_both_expected_revisions_and_identity() {
    let fixture = fixture();
    let valid = serde_json::json!({
        "api_version":"v1","session_id":"session-cancel","goal_id":"goal-cancel",
        "revision":3,"state":"cancelled","session_version":6,"committed_position":8
    });
    let invalid = serde_json::json!({
        "api_version":"v1","session_id":"session-cancel","goal_id":"goal-cancel",
        "revision":3,"state":"failed","session_version":6,"committed_position":8
    });
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, &valid.to_string()),
            http_json(200, &invalid.to_string()),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).unwrap();
    let response = client
        .cancel_goal(
            "cancel-goal",
            "session-cancel",
            "goal-cancel",
            5,
            2,
            "operator_cancelled",
        )
        .await
        .unwrap();
    assert_eq!(response.state, "cancelled");
    assert_eq!(
        client
            .cancel_goal(
                "cancel-goal-2",
                "session-cancel",
                "goal-cancel",
                5,
                2,
                "operator_cancelled",
            )
            .await
            .unwrap_err()
            .code,
        HostClientErrorCode::InvalidEvent
    );
    server.await.unwrap();
    let request = &requests.lock().await[0];
    assert!(request.starts_with("POST /v1/goals/goal-cancel:cancel HTTP/1.1\r\n"));
    assert!(request.contains("\"expected_session_version\":5"));
    assert!(request.contains("\"expected_revision\":2"));
}

#[tokio::test]
async fn goal_revise_binds_canonical_definition_and_both_revisions() {
    let fixture = fixture();
    let definition = goal_definition_json("goal-revise", "refined", "session-revise");
    let valid = serde_json::json!({
        "api_version":"v1","session_id":"session-revise","goal_id":"goal-revise",
        "revision":3,"state":"draft","session_version":6,"committed_position":8
    });
    let invalid = serde_json::json!({
        "api_version":"v1","session_id":"session-revise","goal_id":"goal-revise",
        "revision":3,"state":"active","session_version":6,"committed_position":8
    });
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, &valid.to_string()),
            http_json(200, &invalid.to_string()),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).unwrap();
    let response = client
        .revise_goal(
            "revise-goal",
            "session-revise",
            "goal-revise",
            5,
            2,
            &definition,
            "objective_refined",
        )
        .await
        .unwrap();
    assert_eq!(response.state, "draft");
    assert_eq!(
        client
            .revise_goal(
                "revise-goal-2",
                "session-revise",
                "goal-revise",
                5,
                2,
                &definition,
                "objective_refined",
            )
            .await
            .unwrap_err()
            .code,
        HostClientErrorCode::InvalidEvent
    );
    server.await.unwrap();
    let request = &requests.lock().await[0];
    assert!(request.starts_with("POST /v1/goals/goal-revise:revise HTTP/1.1\r\n"));
    assert!(request.contains("\"replacement_reason\":\"objective_refined\""));
}

#[tokio::test]
async fn plan_page_is_typed_ordered_and_count_checked() {
    let fixture = fixture();
    let plan = serde_json::json!({
        "api_version":"v1","plan_id":"plan-a","revision":1,"state":"running",
        "definition_digest":"a".repeat(64),"goal_id":"goal-a","goal_revision":2,
        "state_version":4,"steps_total":3,"steps_ready":1,"steps_active":1,
        "steps_completed":1,"steps_failed":0,"total_attempts":2
    });
    let valid = serde_json::json!({
        "api_version":"v1","session_id":"session-plans","plans":[plan.clone()],
        "session_version":4,"observed_max_position":4
    });
    let mut invalid_plan = plan;
    invalid_plan["steps_failed"] = serde_json::json!(2);
    let invalid = serde_json::json!({
        "api_version":"v1","session_id":"session-plans","plans":[invalid_plan],
        "session_version":4,"observed_max_position":4
    });
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, &valid.to_string()),
            http_json(200, &invalid.to_string()),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");

    let page = client.get_plans("session-plans").await.expect("plans");
    assert_eq!(page.plans[0].state, "running");
    assert_eq!(
        client.get_plans("session-plans").await.unwrap_err().code,
        HostClientErrorCode::InvalidEvent
    );
    server.await.expect("server task");
    assert!(
        requests.lock().await[0].starts_with("GET /v1/sessions/session-plans/plans HTTP/1.1\r\n")
    );
}

#[tokio::test]
async fn live_delta_accepts_max_raw_text_after_json_escaping() {
    let fixture = fixture();
    let text = "\0".repeat(32 * 1_024);
    let body = serde_json::json!({
        "api_version": "v1",
        "session_id": "session-live-bounds",
        "turn_id": "turn-live-bounds",
        "execution_id": "execution-live-bounds",
        "stream_id": "12345678-1234-4234-8234-123456789abc",
        "sequence": 1,
        "kind": "text_delta",
        "text": text
    });
    let encoded = serde_json::to_string(&body).unwrap();
    assert!(encoded.len() > fixture.limits.max_event_bytes);
    assert!(encoded.len() <= 6 * 32 * 1_024 + 2_048);

    let response = http_live_sse(&encoded);
    let (base_url, server) = serve(vec![response], Arc::new(Mutex::new(Vec::new()))).await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let follow = tokio::spawn(async move {
        client
            .follow_live_output("session-live-bounds", sender)
            .await
    });

    let delta = receiver.recv().await.expect("delta");
    assert!(matches!(
        delta.kind,
        LiveOutputEventKind::TextDelta { text: received } if received == text
    ));
    assert_eq!(
        follow.await.unwrap().unwrap_err().code,
        HostClientErrorCode::TransportFailure
    );
    server.await.expect("server task");
}

#[tokio::test]
async fn live_delta_rejects_raw_text_above_h4_bound() {
    let fixture = fixture();
    let body = serde_json::json!({
        "api_version": "v1",
        "session_id": "session-live-bounds",
        "turn_id": "turn-live-bounds",
        "execution_id": "execution-live-bounds",
        "stream_id": "12345678-1234-4234-8234-123456789abc",
        "sequence": 1,
        "kind": "text_delta",
        "text": "x".repeat(32 * 1_024 + 1)
    });
    let response = http_live_sse(&serde_json::to_string(&body).unwrap());
    let (base_url, server) = serve(vec![response], Arc::new(Mutex::new(Vec::new()))).await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);

    let error = client
        .follow_live_output("session-live-bounds", sender)
        .await
        .expect_err("oversized raw delta");

    assert_eq!(error.code, HostClientErrorCode::InvalidEvent);
    server.await.expect("server task");
}

#[tokio::test]
async fn host_fixture_errors_are_typed_without_body_disclosure() {
    let fixture = fixture();
    for host_error in &fixture.host_errors {
        let body = serde_json::json!({"code": host_error.code, "secret": "must-not-leak"});
        let responses = vec![http_json(host_error.status, &body.to_string())];
        let (base_url, server) = serve(responses, Arc::new(Mutex::new(Vec::new()))).await;
        let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
        let error = client
            .create_session("stable", "definition")
            .await
            .expect_err("host error");
        server.await.expect("server task");
        assert_eq!(error.code, HostClientErrorCode::HostFailure);
        assert_eq!(error.status, Some(host_error.status));
        assert!(!format!("{error:?} {error}").contains("must-not-leak"));
    }

    for (status, code, expected) in [
        (
            401,
            "authentication_required",
            HostClientErrorCode::AuthenticationRequired,
        ),
        (403, "actor_forbidden", HostClientErrorCode::ActorForbidden),
        (
            409,
            "device_reauth_required",
            HostClientErrorCode::DeviceReauthRequired,
        ),
        (429, "rate_limited", HostClientErrorCode::RateLimited),
        (
            503,
            "runtime_unavailable",
            HostClientErrorCode::RuntimeUnavailable,
        ),
        (
            401,
            "pairing_rejected",
            HostClientErrorCode::PairingRejected,
        ),
    ] {
        let body = serde_json::json!({"code": code, "secret": "must-not-leak"});
        let responses = vec![http_json(status, &body.to_string())];
        let (base_url, server) = serve(responses, Arc::new(Mutex::new(Vec::new()))).await;
        let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
        let error = client
            .create_session("stable", "definition")
            .await
            .expect_err("gateway error");
        server.await.expect("server task");
        assert_eq!(error.code, expected);
        assert_eq!(error.status, Some(status));
        assert!(!format!("{error:?} {error}").contains("must-not-leak"));
    }
}

#[tokio::test]
async fn mutation_methods_use_exact_h1_paths_and_bodies() {
    let fixture = fixture();
    let response = r#"{"session_id":"session-client","turn_id":"turn-client","execution_id":"execution-client","committed_position":12}"#;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, response),
            http_json(200, response),
            http_json(200, response),
            http_json(200, response),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");
    let invalid = client
        .ask_reply_event(
            "invalid-json",
            "session-client",
            "turn-client",
            "suspension-client",
            4,
            &fixture.typed_continuation.non_canonical_json,
        )
        .await
        .expect_err("non-canonical JSON");
    assert_eq!(invalid.code, HostClientErrorCode::InvalidCommand);
    assert_eq!(
        invalid.code.wire_name(),
        fixture.typed_continuation.non_canonical_error
    );
    client
        .cancel_turn("cancel-stable", "session-client", "turn-client", 9)
        .await
        .expect("cancel");
    client
        .approval_event(
            "approval-stable",
            "session-client",
            "turn-client",
            "suspension-client",
            4,
            ApprovalDecision::Approve,
        )
        .await
        .expect("approval");
    client
        .external_input_event(
            "external-input-stable",
            "session-client",
            "turn-client",
            "suspension-client",
            4,
            "approved input",
        )
        .await
        .expect("external input");
    client
        .ask_reply_event(
            "ask-reply-stable",
            "session-client",
            "turn-client",
            "suspension-client",
            4,
            &fixture.typed_continuation.canonical_json,
        )
        .await
        .expect("ask reply");
    server.await.expect("server task");
    let requests = requests.lock().await;
    assert!(
        requests[0]
            .starts_with("POST /v1/sessions/session-client/turns/turn-client/cancel HTTP/1.1\r\n"),
        "cancel path must be session-scoped, got: {}",
        requests[0]
    );
    assert!(requests[0].contains("\"requested_through_position\":9"));
    assert!(requests[1]
        .starts_with("POST /v1/sessions/session-client/turns/turn-client/events HTTP/1.1\r\n"));
    assert!(requests[1].contains("\"kind\":\"approval\""));
    assert!(requests[1].contains("\"expected_session_version\":4"));
    assert!(requests[1].contains("\"suspension_id\":\"suspension-client\""));
    assert!(requests[1].contains("\"decision\":\"approve\""));
    assert!(requests[2].contains("\"kind\":\"external_input\""));
    assert!(requests[2].contains("\"text\":\"approved input\""));
    assert!(requests[3].contains("\"kind\":\"ask_reply\""));
    assert!(
        requests[3].contains("\"input_json\":\"{\\\"approved\\\":true}\""),
        "ask_reply body must carry the exact RFC 8785 JSON bytes, got: {}",
        requests[3]
    );
    assert!(!requests[3].contains("\"text\""));
}

#[tokio::test]
async fn membership_and_broadcast_use_explicit_session_contracts() {
    let fixture = fixture();
    let roster = r#"{"api_version":"v1","session_id":"session-client","members":[{"agent_id":"alpha","joined_position":2}],"observed_max_position":2}"#;
    let broadcast = r#"{"api_version":"v1","session_id":"session-client","delivery":"broadcast","turns":[{"agent_id":"alpha","turn_id":"turn-alpha","execution_id":"execution-alpha","committed_position":5}]}"#;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (base_url, server) = serve(
        vec![
            http_json(200, roster),
            http_json(200, roster),
            http_json(200, roster),
            http_json(200, broadcast),
        ],
        Arc::clone(&requests),
    )
    .await;
    let client = LiveHostClient::new(&base_url, limits(&fixture)).expect("client");

    assert_eq!(
        client
            .get_session_membership("session-client")
            .await
            .unwrap()
            .members[0]
            .agent_id,
        "alpha"
    );
    client
        .add_session_agent("join-alpha", "session-client", "alpha")
        .await
        .unwrap();
    client
        .remove_session_agent("leave-alpha", "session-client", "alpha")
        .await
        .unwrap();
    let started = client
        .start_turn_broadcast("broadcast-alpha", "session-client", "hello")
        .await
        .unwrap();
    assert_eq!(started.turns[0].agent_id, "alpha");
    server.await.unwrap();

    let requests = requests.lock().await;
    assert!(requests[0].starts_with("GET /v1/sessions/session-client/agents HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("POST /v1/sessions/session-client/agents HTTP/1.1\r\n"));
    assert!(requests[1].contains("\"agent_id\":\"alpha\""));
    assert!(requests[2].starts_with("DELETE /v1/sessions/session-client/agents/alpha HTTP/1.1\r\n"));
    assert!(requests[3].contains("\"delivery\":\"broadcast\""));
    assert!(!requests[3].contains("\"agent_id\""));
}

fn goal_definition_json(goal_id: &str, objective: &str, session_id: &str) -> String {
    serde_jcs::to_string(&serde_json::json!({
        "bounds":{"duration_budget_ms":null,"max_attempts":1,"max_child_goals":1,"max_plan_revisions":1,"token_budget":null},
        "capability_references":[],"contract":"garive.goal-definition",
        "criteria":[{"criterion_id":"accepted","kind":"user_acceptance","response_schema_digest":"a".repeat(64)}],
        "goal_id":goal_id,"objective":objective,"parent_goal_id":null,
        "scope":{"session_id":session_id,"workspace_capability_ids":[]},"version":1
    }))
    .unwrap()
}

fn http_json(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn http_sse(events: &[HostEvent]) -> Vec<u8> {
    let body = events
        .iter()
        .map(|event| format!("data: {}\n\n", serde_json::to_string(event).unwrap()))
        .collect::<String>();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn http_live_sse(event: &str) -> Vec<u8> {
    let body = format!("event: live\ndata: {event}\n\n");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn serve(
    responses: Vec<Vec<u8>>,
    requests: Arc<Mutex<Vec<String>>>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![0; 16_384];
            let read = socket.read(&mut bytes).await.unwrap();
            requests
                .lock()
                .await
                .push(String::from_utf8_lossy(&bytes[..read]).into_owned());
            socket.write_all(&response).await.unwrap();
        }
    });
    (format!("http://{address}/"), task)
}
