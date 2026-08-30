use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use futures::StreamExt;
use garive_core::{
    AgentOutcome, ExecutionReport, GovernedSuspensionBinding, SuspensionReason, UsageSummary,
};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId, ToolInvocationId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, ActivityProjectionLimits, CommittedTurn, CoreTerminalContext,
    EffectiveRuntimeLimits, HostClock, HostContinuationInput, HostReadLimits,
    InstalledActivityCatalogue, InstalledActivityDescriptor, InstalledAgent, LiveHost,
    LiveHostError, LiveHostLimits, LiveHostServer, SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::oneshot;

const NOW: &str = "2026-08-29T00:00:00Z";

struct FixedClock;

impl HostClock for FixedClock {
    fn recorded_at(&self) -> String {
        NOW.to_owned()
    }
}

struct VerifyingDispatcher {
    database: PathBuf,
    committed: Mutex<Vec<CommittedTurn>>,
}

impl TurnDispatcher for VerifyingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let ledger = SqliteLedger::open(&self.database).unwrap();
        let watermark = ledger.session_watermark(&turn.session_id).unwrap().unwrap();
        assert!(watermark.max_position >= turn.committed_position);
        self.committed.lock().unwrap().push(turn.clone());
        Ok(())
    }
}

struct Harness {
    _directory: TempDir,
    database: PathBuf,
    dispatcher: Arc<VerifyingDispatcher>,
    host: LiveHost,
}

impl Harness {
    fn new(event_batch_size: u64) -> Self {
        Self::with_h3(event_batch_size, false)
    }

    fn h3(event_batch_size: u64) -> Self {
        Self::with_h3(event_batch_size, true)
    }

    fn with_h3(event_batch_size: u64, h3: bool) -> Self {
        Self::with_read_limits(event_batch_size, h3, HostReadLimits::PRODUCT_DEFAULT)
    }

    fn with_read_limits(event_batch_size: u64, h3: bool, read_limits: HostReadLimits) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("host.sqlite3");
        let dispatcher = Arc::new(VerifyingDispatcher {
            database: database.clone(),
            committed: Mutex::new(Vec::new()),
        });
        let mut installed = installed();
        installed.public_activity_catalogue = h3.then(activity_catalogue);
        let host = LiveHost::new_with_read_limits(
            &database,
            installed,
            LiveHostLimits {
                max_command_bytes: 4_096,
                event_batch_size,
                event_poll_interval_ms: 10,
                activity: h3.then_some(ActivityProjectionLimits {
                    max_activities_per_turn: 8,
                    max_activity_facts: 64,
                    max_label_bytes: 128,
                    max_activity_id_bytes: 128,
                    max_encoded_bytes_per_turn: 8_192,
                }),
            },
            read_limits,
            Arc::new(FixedClock),
            dispatcher.clone(),
        )
        .unwrap();
        Self {
            _directory: directory,
            database,
            dispatcher,
            host,
        }
    }
}

#[test]
fn h2_read_limits_fail_closed_and_truncate_only_display_text() {
    let text_limits = HostReadLimits {
        max_user_text_bytes: 5,
        ..HostReadLimits::PRODUCT_DEFAULT
    };
    let harness = Harness::with_read_limits(64, false, text_limits);
    let session = harness
        .host
        .create_session("create-text-bound", "definition-main")
        .unwrap();
    harness
        .host
        .start_turn("start-text-bound", &session.session_id, "ééé")
        .unwrap();
    let page = harness
        .host
        .get_timeline(&session.session_id, 0, 4)
        .unwrap();
    assert_eq!(page.items[0].user_text, "éé");
    assert!(page.items[0].content_truncated);

    let response_bound = Harness::with_read_limits(
        64,
        false,
        HostReadLimits {
            max_response_bytes: 1,
            ..HostReadLimits::PRODUCT_DEFAULT
        },
    );
    assert_eq!(
        response_bound.host.list_agent_definitions(),
        Err(LiveHostError::ReadBoundExceeded)
    );

    let fact_bound = Harness::with_read_limits(
        64,
        false,
        HostReadLimits {
            max_facts: 2,
            ..HostReadLimits::PRODUCT_DEFAULT
        },
    );
    let session = fact_bound
        .host
        .create_session("create-fact-bound", "definition-main")
        .unwrap();
    fact_bound
        .host
        .start_turn("start-fact-bound", &session.session_id, "hello")
        .unwrap();
    assert_eq!(
        fact_bound.host.get_session(&session.session_id),
        Err(LiveHostError::ReadBoundExceeded)
    );
}

fn activity_catalogue() -> InstalledActivityCatalogue {
    InstalledActivityCatalogue {
        schema_version: 1,
        catalogue_revision: "activity-labels-1".into(),
        descriptors: vec![InstalledActivityDescriptor {
            tool_name: "read_file".into(),
            tool_revision: "1".into(),
            label_key: "agent.activity.read_file".into(),
        }],
    }
}

fn installed() -> InstalledAgent {
    InstalledAgent {
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        agent_instance_namespace: "installed-main".into(),
        public_capabilities: vec!["timeline".into(), "tools".into()],
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(1_024),
            max_output_tokens: Some(512),
            deadline_budget_ms: Some(30_000),
        },
        public_activity_catalogue: None,
    }
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/host/live-host-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn commands_are_durable_idempotent_and_dispatched_only_after_commit() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    assert_eq!(session.committed_position, 1);
    assert_eq!(
        harness
            .host
            .create_session("create-1", "definition-main")
            .unwrap(),
        session
    );
    assert_eq!(
        harness
            .host
            .start_turn("create-1", &session.session_id, "hello")
            .unwrap_err(),
        LiveHostError::CommandConflict
    );

    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();
    assert_eq!(started.committed_position, 4);
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 1);

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher.clone(),
    )
    .unwrap();
    assert_eq!(
        restarted
            .start_turn("start-1", &session.session_id, "hello")
            .unwrap(),
        started
    );
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 1);
    assert_eq!(
        restarted
            .start_turn("start-1", &session.session_id, "different")
            .unwrap_err(),
        LiveHostError::CommandConflict
    );
    let id = SessionId::try_from(session.session_id.as_str()).unwrap();
    assert_eq!(
        SqliteLedger::open(&harness.database)
            .unwrap()
            .session_watermark(&id)
            .unwrap()
            .unwrap()
            .max_position,
        4
    );
}

#[test]
fn installed_definitions_and_sessions_are_restart_safe_read_models() {
    let harness = Harness::new(64);
    let definitions = harness.host.list_agent_definitions().unwrap();
    assert_eq!(definitions.definitions.len(), 1);
    assert_eq!(definitions.definitions[0].api_version, "v1");
    assert_eq!(definitions.definitions[0].definition_id, "definition-main");
    assert_eq!(
        definitions.definitions[0].capabilities,
        ["timeline", "tools"]
    );
    assert_eq!(definitions.definitions[0].definition_revision, "revision-1");

    let first = harness
        .host
        .create_session("create-read-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-read-1", &first.session_id, "durable input")
        .unwrap();
    let second = harness
        .host
        .create_session("create-read-2", "definition-main")
        .unwrap();

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher,
    )
    .unwrap();
    let sessions = restarted.list_sessions(2, None).unwrap().sessions;
    assert_eq!(sessions.len(), 2);
    assert!(sessions[0].session_id > sessions[1].session_id);
    let active = sessions
        .iter()
        .find(|summary| summary.session_id == first.session_id)
        .unwrap();
    assert_eq!(
        active.latest_turn_id.as_deref(),
        Some(started.turn_id.as_str())
    );
    assert_eq!(active.latest_turn_state.as_deref(), Some("running"));
    assert_eq!(active.turn_count, 1);
    assert_eq!(active.latest_position, 4);
    assert_eq!(active.opened_at, NOW);
    assert!(sessions
        .iter()
        .any(|summary| summary.session_id == second.session_id));
    assert_eq!(
        restarted.list_sessions(0, None),
        Err(LiveHostError::InvalidRequest)
    );
}

#[test]
fn session_view_tracks_first_starts_and_latest_lifecycle() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-view", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-view", &session.session_id, "hello")
        .unwrap();
    let running = harness.host.get_session(&session.session_id).unwrap();
    assert_eq!(running.api_version, "v1");
    assert_eq!(running.session.turn_count, 1);
    assert_eq!(
        running.session.latest_turn_id.as_deref(),
        Some(started.turn_id.as_str())
    );
    assert_eq!(
        running.session.latest_turn_state.as_deref(),
        Some("running")
    );
    assert_eq!(running.observed_max_position, started.committed_position);
    assert_eq!(running.session.opened_at, NOW);
}

#[test]
fn timeline_pages_complete_turns_by_latest_change_without_splitting() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("timeline-session", "definition-main")
        .unwrap();
    let first = harness
        .host
        .start_turn("timeline-first", &session.session_id, "first")
        .unwrap();
    let second = harness
        .host
        .start_turn("timeline-second", &session.session_id, "second")
        .unwrap();

    let page = harness
        .host
        .get_timeline(&session.session_id, 0, 1)
        .unwrap();
    assert_eq!(page.api_version, "v1");
    assert_eq!(page.observed_max_position, 7);
    assert_eq!(page.scanned_through_position, 3);
    assert!(page.has_more);
    assert_eq!(page.items[0].turn_id, first.turn_id);
    assert_eq!(page.items[0].user_text, "first");
    assert_eq!(page.items[0].state, "running");

    let next = harness
        .host
        .get_timeline(&session.session_id, page.scanned_through_position, 1)
        .unwrap();
    assert!(!next.has_more);
    assert_eq!(next.scanned_through_position, 7);
    assert_eq!(next.items[0].turn_id, second.turn_id);
    assert_eq!(next.items[0].user_text, "second");
    assert_eq!(
        harness.host.get_timeline(&session.session_id, 8, 1),
        Err(LiveHostError::InvalidRequest)
    );
}

#[test]
fn session_pages_use_stable_checked_cursors() {
    let harness = Harness::new(64);
    let first = harness
        .host
        .create_session("page-a", "definition-main")
        .unwrap();
    let second = harness
        .host
        .create_session("page-b", "definition-main")
        .unwrap();
    let page_one = harness.host.list_sessions(1, None).unwrap();
    assert_eq!(page_one.sessions.len(), 1);
    assert_eq!(page_one.sessions[0].session_id, second.session_id);
    let cursor = page_one.next_before.as_deref().unwrap();
    let page_two = harness.host.list_sessions(1, Some(cursor)).unwrap();
    assert_eq!(page_two.sessions[0].session_id, first.session_id);
    assert!(page_two.next_before.is_none());

    let mut corrupt = cursor.as_bytes().to_vec();
    let last = corrupt.len() - 1;
    corrupt[last] = if corrupt[last] == b'A' { b'B' } else { b'A' };
    assert_eq!(
        harness
            .host
            .list_sessions(1, std::str::from_utf8(&corrupt).ok())
            .unwrap_err(),
        LiveHostError::InvalidRequest
    );
}

#[test]
fn event_projection_advances_over_gaps_and_replays_terminal_text() {
    let harness = Harness::new(1);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();

    let first = harness
        .host
        .read_event_page(&session.session_id, 0)
        .unwrap();
    assert_eq!(first.events[0].event, "session.created");
    let second = harness
        .host
        .read_event_page(&session.session_id, 1)
        .unwrap();
    assert_eq!(second.events[0].event, "turn.started");
    let hidden_input = harness
        .host
        .read_event_page(&session.session_id, 2)
        .unwrap();
    assert!(hidden_input.events.is_empty());
    assert_eq!(hidden_input.scanned_through_position, 3);
    let hidden_execution = harness
        .host
        .read_event_page(&session.session_id, 3)
        .unwrap();
    assert!(hidden_execution.events.is_empty());
    assert_eq!(hidden_execution.scanned_through_position, 4);

    let usage = UsageSummary {
        input_tokens: TokenCount::Known(2),
        output_tokens: TokenCount::Known(1),
        estimated: false,
    };
    let report = ExecutionReport {
        outcome: AgentOutcome::Completed {
            response_items: vec![
                ModelItem::Text { text: "do".into() },
                ModelItem::Refusal { text: "ne".into() },
            ],
            usage,
        },
        completed_iterations: 1,
        usage,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap(),
            execution_id: garive_ledger::ExecutionId::try_from(started.execution_id.as_str())
                .unwrap(),
            recorded_at: NOW.into(),
        },
        &report,
    )
    .unwrap();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(
            SessionId::try_from(session.session_id.as_str()).unwrap(),
            2,
            terminal,
        )
        .unwrap();
    let hidden_terminal = harness
        .host
        .read_event_page(&session.session_id, 4)
        .unwrap();
    assert!(hidden_terminal.events.is_empty());
    let completed = harness
        .host
        .read_event_page(&session.session_id, 5)
        .unwrap();
    assert_eq!(completed.events[0].event, "turn.completed");
    assert_eq!(completed.events[0].text, "done");
    let timeline = harness
        .host
        .get_timeline(&session.session_id, 0, 10)
        .unwrap();
    assert_eq!(timeline.api_version, "v1");
    assert_eq!(timeline.items.len(), 1);
    assert_eq!(timeline.items[0].turn_id, started.turn_id);
    assert_eq!(timeline.items[0].user_text, "hello");
    assert_eq!(timeline.items[0].state, "completed");
    assert_eq!(timeline.items[0].completion_text.as_deref(), Some("done"));
    assert!(!timeline.items[0].content_truncated);
    assert_eq!(timeline.observed_max_position, 6);
    assert!(!timeline.has_more);

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher,
    )
    .unwrap();
    assert_eq!(
        restarted.read_event_page(&session.session_id, 5).unwrap(),
        completed
    );
    assert_eq!(
        restarted.get_timeline(&session.session_id, 0, 10).unwrap(),
        timeline
    );
}

#[test]
fn cancellation_is_a_replayable_request_not_a_terminal_claim() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();
    let cancelled = harness
        .host
        .cancel_turn("cancel-1", &session.session_id, &started.turn_id, 4)
        .unwrap();
    assert_eq!(cancelled.committed_position, 5);
    assert_eq!(
        harness
            .host
            .cancel_turn("cancel-1", &session.session_id, &started.turn_id, 4)
            .unwrap(),
        cancelled
    );
    assert_eq!(
        harness
            .host
            .cancel_turn("cancel-1", &session.session_id, &started.turn_id, 3)
            .unwrap_err(),
        LiveHostError::CommandConflict
    );
}

#[test]
fn continuation_replay_binds_suspension_input_and_expected_version() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-1", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-1", &session.session_id, "hello")
        .unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        estimated: false,
    };
    let suspended = ExecutionReport {
        outcome: AgentOutcome::Suspended {
            reason: SuspensionReason::PartialOutput,
            partial_items: vec![ModelItem::Text {
                text: "partial".into(),
            }],
            last_durable_position: 4,
            governed_binding: None,
        },
        completed_iterations: 1,
        usage,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap(),
            execution_id: garive_ledger::ExecutionId::try_from(started.execution_id.as_str())
                .unwrap(),
            recorded_at: NOW.into(),
        },
        &suspended,
    )
    .unwrap();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(
            SessionId::try_from(session.session_id.as_str()).unwrap(),
            2,
            terminal,
        )
        .unwrap();
    let ledger = SqliteLedger::open(&harness.database).unwrap();
    let snapshot = ledger
        .load_turn(&garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap())
        .unwrap();
    let state = garive_runtime::reconstruct_suspended_turn(&snapshot).unwrap();
    let timeline = harness
        .host
        .get_timeline(&session.session_id, 0, 10)
        .unwrap();
    let public = timeline.items[0].suspension.as_ref().unwrap();
    assert_eq!(public.suspension_id, state.suspension_id);
    assert_eq!(public.kind, "partial_output");
    assert_eq!(public.session_version, 3);
    assert!(public.response_schema_json.is_none());
    assert!(public.prompt_json.contains("suspension.partial_output"));
    let continued = harness
        .host
        .continue_turn(
            "continue-1",
            &session.session_id,
            &started.turn_id,
            &state.suspension_id,
            3,
            HostContinuationInput::String("more"),
        )
        .unwrap();
    assert_eq!(continued.committed_position, 9);
    let timeline = harness
        .host
        .get_timeline(&session.session_id, 0, 10)
        .unwrap();
    assert_eq!(timeline.items[0].state, "running");
    assert!(timeline.items[0].suspension.is_none());
    assert_eq!(timeline.items[0].user_text, "hello");

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher,
    )
    .unwrap();
    assert_eq!(
        restarted
            .continue_turn(
                "continue-1",
                &session.session_id,
                &started.turn_id,
                &state.suspension_id,
                3,
                HostContinuationInput::String("more"),
            )
            .unwrap(),
        continued
    );
    assert_eq!(
        restarted
            .continue_turn(
                "continue-1",
                &session.session_id,
                &started.turn_id,
                &state.suspension_id,
                4,
                HostContinuationInput::String("more"),
            )
            .unwrap_err(),
        LiveHostError::CommandConflict
    );
}

#[test]
fn h3_projects_committed_effects_into_events_and_restart_safe_timeline() {
    let harness = Harness::h3(64);
    let session = harness
        .host
        .create_session("create-h3", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-h3", &session.session_id, "read the brief")
        .unwrap();
    let session_id = SessionId::try_from(session.session_id.as_str()).unwrap();
    let turn_id = garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap();
    let execution_id = garive_ledger::ExecutionId::try_from(started.execution_id.as_str()).unwrap();
    let tool_id = ToolInvocationId::try_from("tool-h3").unwrap();
    let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let binding = |value: Value| {
        let content = CanonicalPayload::from_value(&value).unwrap();
        serde_json::json!({"digest":content.sha256(),"inline_utf8":content.as_json()})
    };
    let values = [
        (
            "h3-prepared",
            "effect.prepared",
            serde_json::json!({"prepared_digest":digest,"tool_name":"read_file","tool_revision":"1","replay_class":"read_only","model_call_id":"call-h3"}),
        ),
        (
            "h3-authorized",
            "effect.authorized",
            serde_json::json!({"prepared_digest":digest,"grant_id":"grant-h3","authority_revision":"policy-1","granted_requirements":binding(serde_json::json!({}))}),
        ),
        (
            "h3-started",
            "effect.started",
            serde_json::json!({"prepared_digest":digest,"grant_id":"grant-h3","executor_id":"local.read","executor_revision":"1","dispatch_attempt_id":"dispatch-h3"}),
        ),
        (
            "h3-receipt",
            "effect.receipt",
            serde_json::json!({"receipt_id":"receipt-h3","prepared_digest":digest,"grant_id":"grant-h3","executor_id":"local.read","executor_revision":"1","classification":"completed","result_or_evidence":binding(serde_json::json!({"ok":true}))}),
        ),
        (
            "h3-completed",
            "effect.completed",
            serde_json::json!({"prepared_digest":digest,"receipt_id":"receipt-h3","result":binding(serde_json::json!({"ok":true}))}),
        ),
        (
            "h3-observation",
            "effect.observation",
            serde_json::json!({"prepared_digest":digest,"model_call_id":"call-h3","observation":binding(serde_json::json!({"ok":true}))}),
        ),
    ];
    let facts = values
        .into_iter()
        .map(|(id, kind, payload)| FactDraft {
            fact_id: FactId::try_from(id).unwrap(),
            turn_id: Some(turn_id.clone()),
            execution_id: Some(execution_id.clone()),
            model_request_id: None,
            tool_invocation_id: Some(tool_id.clone()),
            kind: FactKind::new(kind).unwrap(),
            schema_version: 1,
            payload: CanonicalPayload::from_value(&payload).unwrap(),
            recorded_at: NOW.into(),
        })
        .collect();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(session_id, 2, facts)
        .unwrap();

    let page = harness
        .host
        .read_event_page(&session.session_id, 0)
        .unwrap();
    let activity = page
        .events
        .iter()
        .filter_map(|event| event.activity.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(
        activity
            .iter()
            .map(|item| item.state.as_str())
            .collect::<Vec<_>>(),
        ["prepared", "authorized", "running", "completed"]
    );
    let timeline = harness
        .host
        .get_timeline(&session.session_id, 0, 8)
        .unwrap();
    assert_eq!(timeline.items[0].activities[0].state, "completed");
    assert_eq!(
        timeline.items[0].activities[0].label_key,
        "agent.activity.read_file"
    );

    let restarted = LiveHost::new(
        &harness.database,
        InstalledAgent {
            public_activity_catalogue: Some(activity_catalogue()),
            ..installed()
        },
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher.clone(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(
            restarted
                .read_event_page(&session.session_id, 0)
                .unwrap()
                .events
        )
        .unwrap(),
        serde_json::to_value(page.events).unwrap()
    );
    assert_eq!(
        serde_json::to_value(restarted.get_timeline(&session.session_id, 0, 8).unwrap()).unwrap(),
        serde_json::to_value(timeline).unwrap()
    );
}

#[test]
fn interaction_continuation_validates_schema_and_representation_before_commit() {
    let contract = fixture();
    let json_value = contract["typed_continuation_cases"][1]["value"]
        .as_str()
        .unwrap();
    let schema_mismatch = contract["invalid_typed_continuations"][3]["value"]
        .as_str()
        .unwrap();
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-interaction", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-interaction", &session.session_id, "hello")
        .unwrap();
    let session_id = SessionId::try_from(session.session_id.as_str()).unwrap();
    let turn_id = garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap();
    let execution_id = garive_ledger::ExecutionId::try_from(started.execution_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&harness.database).unwrap();
    ledger
        .commit(
            session_id.clone(),
            2,
            vec![
                FactDraft {
                    fact_id: FactId::try_from("effect-prepared").unwrap(),
                    turn_id: Some(turn_id.clone()),
                    execution_id: Some(execution_id.clone()),
                    model_request_id: None,
                    tool_invocation_id: Some(ToolInvocationId::try_from("tool-1").unwrap()),
                    kind: FactKind::new("effect.prepared").unwrap(),
                    schema_version: 1,
                    payload: CanonicalPayload::from_value(&serde_json::json!({
                        "prepared_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                        "tool_name":"tool",
                        "tool_revision":"revision",
                        "replay_class":"never_replay",
                        "model_call_id":"call-1"
                    }))
                    .unwrap(),
                    recorded_at: NOW.into(),
                },
                FactDraft {
                fact_id: FactId::try_from("interaction-requested").unwrap(),
                turn_id: Some(turn_id.clone()),
                execution_id: Some(execution_id.clone()),
                model_request_id: None,
                tool_invocation_id: Some(ToolInvocationId::try_from("tool-1").unwrap()),
                kind: FactKind::new("interaction.requested").unwrap(),
                schema_version: 1,
                payload: CanonicalPayload::from_value(&serde_json::json!({
                    "interaction_id":"interaction-1",
                    "suspension_id":"suspension-1",
                    "prepared_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "kind":"approval",
                    "prompt":{"digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","inline_utf8":""},
                    "response_schema":{"digest":"7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553","inline_utf8":"{\"type\":\"boolean\"}"},
                    "response_schema_digest":"7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553",
                    "expiry_code":"none"
                }))
                .unwrap(),
                recorded_at: NOW.into(),
                },
            ],
        )
        .unwrap();
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn_id.clone(),
            execution_id,
            recorded_at: NOW.into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Suspended {
                reason: SuspensionReason::ApprovalRequired,
                partial_items: vec![],
                last_durable_position: 6,
                governed_binding: Some(GovernedSuspensionBinding::Interaction {
                    suspension_id: "suspension-1".into(),
                    interaction_id: "interaction-1".into(),
                    invocation_id: "tool-1".into(),
                    prepared_digest:
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                }),
            },
            completed_iterations: 1,
            usage: UsageSummary {
                input_tokens: TokenCount::Known(1),
                output_tokens: TokenCount::Known(1),
                estimated: false,
            },
        },
    )
    .unwrap();
    ledger.commit(session_id, 3, terminal).unwrap();
    let before = ledger
        .session_watermark(&SessionId::try_from(session.session_id.as_str()).unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(
        harness.host.continue_turn(
            "invalid-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::Json(schema_mismatch)
        ),
        Err(LiveHostError::InvalidRequest)
    );
    let after_invalid = ledger
        .session_watermark(&SessionId::try_from(session.session_id.as_str()).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(before, after_invalid);
    assert_eq!(
        harness.host.continue_turn(
            "noncanonical-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::Json(" true")
        ),
        Err(LiveHostError::InvalidRequest)
    );

    let continued = harness
        .host
        .continue_turn(
            "continue-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::Json(json_value),
        )
        .unwrap();
    assert_eq!(continued.committed_position, 12);
    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher.clone(),
    )
    .unwrap();
    assert_eq!(
        restarted
            .continue_turn(
                "continue-interaction",
                &session.session_id,
                &started.turn_id,
                "suspension-1",
                4,
                HostContinuationInput::Json(json_value),
            )
            .unwrap(),
        continued
    );
    assert_eq!(
        restarted.continue_turn(
            "continue-interaction",
            &session.session_id,
            &started.turn_id,
            "suspension-1",
            4,
            HostContinuationInput::String("true")
        ),
        Err(LiveHostError::CommandConflict)
    );
}

#[tokio::test]
async fn real_loopback_http_has_stable_errors_commands_and_sse_replay() {
    let harness = Harness::new(64);
    let server = LiveHostServer::bind(
        harness.host.clone(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
    )
    .await
    .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let client = reqwest::Client::new();
    let base = format!("http://{address}");

    let missing = client
        .post(format!("{base}/v1/sessions"))
        .header("content-type", "application/json")
        .body(r#"{"agent_definition_id":"definition-main"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::BAD_REQUEST);
    let missing: Value = serde_json::from_slice(&missing.bytes().await.unwrap()).unwrap();
    assert_eq!(missing["code"], "invalid_request");

    for (key, body) in [
        (
            "continue-absent",
            r#"{"session_id":"session-x","suspension_id":"suspension-x","expected_session_version":1}"#,
        ),
        (
            "continue-dual",
            r#"{"session_id":"session-x","suspension_id":"suspension-x","expected_session_version":1,"input":"yes","input_json":"true"}"#,
        ),
    ] {
        let response = client
            .post(format!("{base}/v1/turns/turn-x:continue"))
            .header("idempotency-key", key)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    let created = client
        .post(format!("{base}/v1/sessions"))
        .header("idempotency-key", "create-http")
        .header("content-type", "application/json")
        .body(r#"{"agent_definition_id":"definition-main"}"#)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap())
        .unwrap();
    let session_id = created["session_id"].as_str().unwrap();
    let session_view = client
        .get(format!("{base}/v1/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(session_view.status(), reqwest::StatusCode::OK);
    let session_view: Value = serde_json::from_slice(&session_view.bytes().await.unwrap()).unwrap();
    assert_eq!(session_view["api_version"], "v1");
    assert_eq!(session_view["session"]["turn_count"], 0);
    assert_eq!(session_view["observed_max_position"], 1);
    let sessions = client
        .get(format!("{base}/v1/sessions?limit=20"))
        .send()
        .await
        .unwrap();
    assert_eq!(sessions.status(), reqwest::StatusCode::OK);
    let sessions: Value = serde_json::from_slice(&sessions.bytes().await.unwrap()).unwrap();
    assert_eq!(sessions["sessions"].as_array().unwrap().len(), 1);
    let started = client
        .post(format!("{base}/v1/sessions/{session_id}/turns"))
        .header("idempotency-key", "start-http")
        .header("content-type", "application/json")
        .body(r#"{"text":"hello"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(started.status(), reqwest::StatusCode::OK);
    let timeline = client
        .get(format!(
            "{base}/v1/sessions/{session_id}/timeline?after_position=0&limit=20"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(timeline.status(), reqwest::StatusCode::OK);
    let timeline: Value = serde_json::from_slice(&timeline.bytes().await.unwrap()).unwrap();
    assert_eq!(timeline["items"][0]["user_text"], "hello");
    assert_eq!(timeline["items"][0]["state"], "running");

    let bad_timeline = client
        .get(format!(
            "{base}/v1/sessions/{session_id}/timeline?limit=20&unknown=1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_timeline.status(), reqwest::StatusCode::BAD_REQUEST);

    let response = client
        .get(format!(
            "{base}/v1/sessions/{session_id}/events?after_position=0"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut bytes = response.bytes_stream();
    let first = bytes.next().await.unwrap().unwrap();
    let text = String::from_utf8(first.to_vec()).unwrap();
    assert!(text.contains("event: host"));
    assert!(text.contains("session.created"));
    assert!(text.contains(r#""api_version":"v1""#));
    drop(bytes);

    shutdown_tx.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn server_rejects_non_loopback_addresses() {
    let harness = Harness::new(64);
    let result = LiveHostServer::bind(
        harness.host,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
    )
    .await;
    assert!(matches!(
        result,
        Err(garive_runtime::LiveHostServerError::NonLoopbackAddress)
    ));
}

#[test]
fn shared_fixture_enumerates_every_stable_failure_code() {
    let fixture = fixture();
    let expected = fixture["failure_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    let actual = [
        LiveHostError::InvalidRequest,
        LiveHostError::InvalidRequest,
        LiveHostError::NotFound,
        LiveHostError::NotFound,
        LiveHostError::NotFound,
        LiveHostError::CommandConflict,
        LiveHostError::ConcurrentModification,
        LiveHostError::PreconditionFailed,
        LiveHostError::DurabilityUnavailable,
        LiveHostError::CorruptState,
    ]
    .map(LiveHostError::code);
    assert_eq!(expected, actual);
}
