use std::{
    collections::BTreeSet,
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Barrier, Mutex},
};

use futures::StreamExt;
use garive_core::{
    AgentOutcome, ExecutionReport, GovernedSuspensionBinding, SuspensionReason, UsageSummary,
};
use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalId, GoalScopeV1,
};
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, FactDraft,
    FactId, FactKind, SessionId, ToolInvocationId,
};
use garive_llm::{ModelItem, TokenCount};
use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanStepId, PlanStepV1,
};
use garive_runtime::{
    commit_goal_command, commit_plan_command, commit_planned_turn, plan_core_terminal,
    plan_create_goal, plan_propose_plan, plan_start_plan_proposal_execution,
    ActivityProjectionLimits, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits,
    GoalCommandAuthority, GoalCommandAuthorityError, GoalCommandContext, HostClock,
    HostContinuationInput, HostReadLimits, InstalledActivityCatalogue, InstalledActivityDescriptor,
    InstalledAgent, LiveHost, LiveHostError, LiveHostLimits, LiveHostServer, PlanCommandContext,
    RuntimeCommandId, SqliteLedger, StartPlanProposalExecutionCommand, StartTurnCommand,
    TurnDispatchError, TurnDispatcher,
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

#[derive(Default)]
struct AllowGoalAuthority {
    create_calls: Mutex<u32>,
    transition_calls: Mutex<u32>,
}

impl GoalCommandAuthority for AllowGoalAuthority {
    fn authorize_create(
        &self,
        _session_id: &str,
        _definition: &GoalDefinitionV1,
    ) -> Result<String, GoalCommandAuthorityError> {
        *self.create_calls.lock().unwrap() += 1;
        Ok("actor:local-user".into())
    }

    fn authorize_transition(
        &self,
        _session_id: &str,
        _current: &garive_runtime::GoalRuntimeState,
        _transition: &garive_runtime::GoalRuntimeTransition,
    ) -> Result<String, GoalCommandAuthorityError> {
        *self.transition_calls.lock().unwrap() += 1;
        Ok("actor:local-user".into())
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
        let activity = h3.then_some(ActivityProjectionLimits {
            max_activities_per_turn: 8,
            max_activity_facts: 64,
            max_label_bytes: 128,
            max_activity_id_bytes: 128,
            max_encoded_bytes_per_turn: 8_192,
        });
        Self::with_limits(event_batch_size, activity, read_limits)
    }

    fn with_limits(
        event_batch_size: u64,
        activity: Option<ActivityProjectionLimits>,
        read_limits: HostReadLimits,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("host.sqlite3");
        let dispatcher = Arc::new(VerifyingDispatcher {
            database: database.clone(),
            committed: Mutex::new(Vec::new()),
        });
        let mut installed = installed();
        installed.public_activity_catalogue = activity.map(|_| activity_catalogue());
        let host = LiveHost::new_with_read_limits(
            &database,
            installed,
            LiveHostLimits {
                max_command_bytes: 4_096,
                event_batch_size,
                event_poll_interval_ms: 10,
                activity,
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
fn one_session_admits_ten_equal_named_agents_and_concurrent_turns() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_named_session("create-team", "definition-main", "Atlas")
        .unwrap();
    for index in 1..10 {
        harness
            .host
            .join_session_agent(
                &format!("join-{index}"),
                &session.session_id,
                "definition-main",
                &format!("Peer-{index}"),
            )
            .unwrap();
    }
    let roster = harness
        .host
        .get_session_agents(&session.session_id)
        .unwrap();
    assert_eq!(roster.members.len(), 10);
    assert_eq!(roster.members[0].display_name, "Atlas");
    assert!(harness
        .host
        .join_session_agent(
            "join-overflow",
            &session.session_id,
            "definition-main",
            "Peer-10"
        )
        .is_err());

    for index in 0..roster.members.len() {
        let recipient = (index + 1) % roster.members.len();
        harness
            .host
            .send_session_agent_message(
                &format!("peer-message-{index}"),
                &session.session_id,
                &roster.members[index].agent_instance_id,
                Some(&roster.members[recipient].agent_instance_id),
                &format!("message-{index}-to-{recipient}"),
            )
            .unwrap();
    }
    let delivered = harness
        .host
        .send_session_agent_message(
            "peer-broadcast",
            &session.session_id,
            &roster.members[9].agent_instance_id,
            None,
            "shared-seed",
        )
        .unwrap();
    assert_eq!(delivered.messages.len(), 11);
    assert_eq!(delivered.messages.last().unwrap().text, "shared-seed");
    assert!(delivered
        .messages
        .iter()
        .take(10)
        .enumerate()
        .all(|(index, message)| {
            message.from_agent_instance_id == roster.members[index].agent_instance_id
                && message.to_agent_instance_id.as_deref()
                    == Some(
                        roster.members[(index + 1) % roster.members.len()]
                            .agent_instance_id
                            .as_str(),
                    )
        }));
    assert_eq!(
        harness.host.send_session_agent_message(
            "forged-message",
            &session.session_id,
            "agent-not-in-session",
            None,
            "forged",
        ),
        Err(LiveHostError::PreconditionFailed)
    );
    assert_eq!(
        harness.host.send_session_agent_message(
            "foreign-recipient",
            &session.session_id,
            &roster.members[0].agent_instance_id,
            Some("agent-not-in-session"),
            "forged",
        ),
        Err(LiveHostError::PreconditionFailed)
    );

    for (index, member) in roster.members.iter().take(2).enumerate() {
        harness
            .host
            .start_agent_turn(
                &format!("peer-turn-{index}"),
                &session.session_id,
                &member.agent_instance_id,
                &format!("Contribution {index}"),
            )
            .unwrap();
    }
    let dispatched = harness.dispatcher.committed.lock().unwrap();
    assert_eq!(dispatched.len(), 2);
    assert_ne!(dispatched[0].turn_id, dispatched[1].turn_id);
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

#[test]
fn h2_goal_projection_is_bounded_ordered_and_redacted() {
    let harness = Harness::with_read_limits(
        64,
        false,
        HostReadLimits {
            max_goal_objective_bytes: 5,
            ..HostReadLimits::PRODUCT_DEFAULT
        },
    );
    let created = harness
        .host
        .create_session("create-goal-session", "definition-main")
        .unwrap();
    let session = SessionId::try_from(created.session_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&harness.database).unwrap();
    let parent = plan_create_goal(
        &ledger,
        &session,
        &goal_context("create-parent"),
        goal_definition("goal-parent", "目标目标", None, session.as_str()),
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &parent).unwrap();
    let child = plan_create_goal(
        &ledger,
        &session,
        &goal_context("create-child"),
        goal_definition("goal-child", "child", Some("goal-parent"), session.as_str()),
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 2, &child).unwrap();

    let page = harness.host.get_goals(session.as_str()).unwrap();
    assert_eq!(page.session_version, 3);
    assert_eq!(page.goals.len(), 2);
    assert_eq!(page.goals[0].goal_id, "goal-child");
    assert_eq!(page.goals[0].parent_goal_id.as_deref(), Some("goal-parent"));
    assert_eq!(page.goals[1].objective, "目");
    assert!(page.goals[1].objective_truncated);
    let encoded = serde_json::to_string(&page).unwrap();
    for private in [
        "workspace-1",
        "catalogue-v1",
        "user:fixture",
        "actor_reference",
    ] {
        assert!(!encoded.contains(private));
    }
}

#[test]
fn public_goal_create_requires_authority_and_exactly_replays_without_reauthorization() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-authority-session", "definition-main")
        .unwrap();
    let definition = goal_definition("goal-public", "public objective", None, &session.session_id)
        .canonical_json()
        .unwrap();
    assert_eq!(
        harness
            .host
            .create_goal("create-public-goal", &session.session_id, 1, &definition,),
        Err(LiveHostError::PreconditionFailed)
    );
    let authority = Arc::new(AllowGoalAuthority::default());
    let host = harness.host.clone().with_goal_authority(authority.clone());
    let first = host
        .create_goal("create-public-goal", &session.session_id, 1, &definition)
        .unwrap();
    let replay = host
        .create_goal("create-public-goal", &session.session_id, 1, &definition)
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(*authority.create_calls.lock().unwrap(), 1);
    let changed = goal_definition(
        "goal-public",
        "changed objective",
        None,
        &session.session_id,
    )
    .canonical_json()
    .unwrap();
    assert_eq!(
        host.create_goal("create-public-goal", &session.session_id, 1, &changed,),
        Err(LiveHostError::CommandConflict)
    );
    let revised = host
        .revise_goal(
            "revise-public-goal",
            &session.session_id,
            "goal-public",
            2,
            1,
            &changed,
            "objective_refined",
        )
        .unwrap();
    assert_eq!(revised.revision, 2);
    assert_eq!(revised.state, "draft");
    assert_eq!(
        revised,
        host.revise_goal(
            "revise-public-goal",
            &session.session_id,
            "goal-public",
            2,
            1,
            &changed,
            "objective_refined",
        )
        .unwrap()
    );
    assert_eq!(*authority.transition_calls.lock().unwrap(), 1);
    let cancelled = host
        .cancel_goal(
            "cancel-public-goal",
            &session.session_id,
            "goal-public",
            3,
            2,
            "operator_cancelled",
        )
        .unwrap();
    assert_eq!(cancelled.revision, 3);
    assert_eq!(cancelled.state, "cancelled");
    assert_eq!(
        cancelled,
        host.cancel_goal(
            "cancel-public-goal",
            &session.session_id,
            "goal-public",
            3,
            2,
            "operator_cancelled",
        )
        .unwrap()
    );
    assert_eq!(*authority.transition_calls.lock().unwrap(), 2);
    assert_eq!(
        host.cancel_goal(
            "cancel-public-goal",
            &session.session_id,
            "goal-public",
            3,
            2,
            "changed_reason",
        ),
        Err(LiveHostError::CommandConflict)
    );
    assert!(
        !serde_json::to_string(&host.get_goals(&session.session_id).unwrap())
            .unwrap()
            .contains("actor:local-user")
    );
}

#[test]
fn h2_plan_projection_is_verified_bounded_and_redacted() {
    let harness = Harness::new(64);
    let created = harness
        .host
        .create_session("create-plan-session", "definition-main")
        .unwrap();
    let session = SessionId::try_from(created.session_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&harness.database).unwrap();
    let goal = goal_definition("goal-plan", "private objective", None, session.as_str());
    let goal_digest = goal.digest().unwrap();
    let created_goal =
        plan_create_goal(&ledger, &session, &goal_context("create-plan-goal"), goal).unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created_goal).unwrap();
    let plan = PlanDefinitionV1::new(
        PlanId::new("plan-main").unwrap(),
        1,
        "goal-plan",
        1,
        goal_digest,
        "b".repeat(64),
        "c".repeat(64),
        "safety-v1",
        vec![PlanStepV1::new(
            PlanStepId::new("deliver").unwrap(),
            "private step objective",
            [],
            ["accepted".into()],
            [PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()],
            [],
            1,
        )
        .unwrap()],
        PlanBoundsV1::new(1, 1, 1, None, None).unwrap(),
        &BTreeSet::from(["accepted".into()]),
        &BTreeSet::new(),
        &BTreeSet::from([PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()]),
    )
    .unwrap();
    let proposed = plan_propose_plan(
        &ledger,
        &session,
        &PlanCommandContext {
            command_id: "propose-plan".into(),
            actor_reference: "private:planner".into(),
            recorded_at: NOW.into(),
        },
        plan,
    )
    .unwrap();
    commit_plan_command(&mut ledger, session.clone(), 2, &proposed).unwrap();

    let page = harness.host.get_plans(session.as_str()).unwrap();
    assert_eq!(page.session_version, 3);
    assert_eq!(page.plans.len(), 1);
    assert_eq!(page.plans[0].state, "proposed");
    assert_eq!(page.plans[0].steps_total, 1);
    let encoded = serde_json::to_string(&page).unwrap();
    for private in ["private step objective", "catalogue-v1", "private:planner"] {
        assert!(!encoded.contains(private));
    }
}

#[test]
fn h2_timeline_is_one_consistent_prefix_during_concurrent_commit() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-concurrent-read", "definition-main")
        .unwrap();
    harness
        .host
        .start_turn("start-before-read", &session.session_id, "first")
        .unwrap();
    // Queue mode: while first Turn is Open, a concurrent second start must
    // be rejected with SessionBusy. The concurrent read observes the prefix
    // it was granted at read time, never the post-write position.
    let barrier = Arc::new(Barrier::new(2));
    let writer_host = harness.host.clone();
    let writer_session = session.session_id.clone();
    let writer_barrier = barrier.clone();
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        let result = writer_host.start_turn("start-during-read", &writer_session, "second");
        assert!(matches!(result, Err(LiveHostError::SessionBusy)));
    });
    barrier.wait();
    let concurrent = harness
        .host
        .get_timeline(&session.session_id, 0, 8)
        .unwrap();
    writer.join().unwrap();
    assert!(matches!(concurrent.observed_max_position, 4 | 7));
    assert!(concurrent
        .items
        .iter()
        .all(|item| item.latest_position <= concurrent.observed_max_position));
    assert_eq!(concurrent.items.len(), 1);
}

#[test]
fn queue_mode_rejects_second_start_while_first_open() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-busy", "definition-main")
        .unwrap();
    harness
        .host
        .start_turn("start-first", &session.session_id, "first")
        .unwrap();
    let busy = harness
        .host
        .start_turn("start-second", &session.session_id, "second");
    assert!(matches!(busy, Err(LiveHostError::SessionBusy)));
    // Replaying the same idempotency key still returns the original
    // TurnCommandResponse (no new commit attempted while busy).
    let replay = harness
        .host
        .start_turn("start-first", &session.session_id, "first");
    assert!(replay.is_ok());
}

#[test]
fn steer_mode_appends_under_same_open_turn_id() {
    // Steer is purely ledger-driven: it commits a turn.steered fact sharing
    // the targeted Turn's id and position order naturally interleaves it
    // with whatever plan.* events the worker emits after this point. The
    // active Turn remains Open; no abort, no in-memory inbox.
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-steer", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-steer", &session.session_id, "first")
        .unwrap();
    let steered = harness
        .host
        .steer_turn(
            "steer-1",
            &session.session_id,
            &started.turn_id,
            "additional context",
        )
        .unwrap();
    assert_eq!(started.turn_id, steered.turn_id);
    assert!(
        steered.committed_position > started.committed_position,
        "steer must commit strictly after start",
    );

    // After steering, the Turn remains Open — a second start_turn still hits
    // the queue-mode busy rejection.
    let busy = harness
        .host
        .start_turn("start-second", &session.session_id, "second");
    assert!(matches!(busy, Err(LiveHostError::SessionBusy)));

    // The committed turn.steered fact lives under the targeted turn_id and
    // increments the durable session version.
    let watermark = SqliteLedger::open(&harness.database)
        .unwrap()
        .session_watermark(
            &garive_ledger::SessionId::try_from(session.session_id.as_str()).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(watermark.max_position, steered.committed_position);
}

#[test]
fn steer_mode_rejects_non_open_turn() {
    // Steer into a Suspended / Completed / Failed / Stopped Turn is a
    // precondition failure — the caller must use continue_turn for a
    // suspended Turn or accept that the Turn is terminal otherwise. We
    // drive the Open Turn to Completed by committing a `turn.completed`
    // fact directly through the ledger, mirroring the same pattern used
    // by the rest of the live_host test suite.
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-steer-pre", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-steer-pre", &session.session_id, "first")
        .unwrap();

    // Move the Open Turn to Completed.
    let session_id = SessionId::try_from(session.session_id.as_str()).unwrap();
    let turn_id = garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap();
    let execution_id = garive_ledger::ExecutionId::try_from(started.execution_id.as_str()).unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(0),
        output_tokens: TokenCount::Known(0),
        estimated: true,
    };
    let report = ExecutionReport {
        outcome: AgentOutcome::Completed {
            response_items: vec![],
            usage,
        },
        completed_iterations: 1,
        usage,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn_id.clone(),
            execution_id: execution_id.clone(),
            recorded_at: NOW.into(),
        },
        &report,
    )
    .unwrap();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(session_id.clone(), 2, terminal)
        .unwrap();

    let rejected = harness.host.steer_turn(
        "steer-pre",
        &session.session_id,
        &started.turn_id,
        "should be rejected",
    );
    assert!(matches!(rejected, Err(LiveHostError::PreconditionFailed)));
}

#[test]
fn steer_mode_rejects_empty_or_oversized_inline_text() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-steer-bound", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-steer-bound", &session.session_id, "first")
        .unwrap();

    let empty = harness
        .host
        .steer_turn("steer-empty", &session.session_id, &started.turn_id, "");
    assert!(matches!(empty, Err(LiveHostError::InvalidRequest)));

    let oversized = "x".repeat(8 * 1024);
    let too_big = harness.host.steer_turn(
        "steer-toobig",
        &session.session_id,
        &started.turn_id,
        &oversized,
    );
    assert!(matches!(too_big, Err(LiveHostError::InvalidRequest)));
}

#[test]
fn steer_mode_is_idempotent_under_replay() {
    // Replaying the same idempotency key for a steer command returns the
    // same TurnCommandResponse without writing a second turn.steered fact —
    // this is the same guarantee we already enforce for start_turn and
    // cancel_turn.
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-steer-replay", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-steer-replay", &session.session_id, "first")
        .unwrap();
    let first = harness
        .host
        .steer_turn(
            "steer-replay",
            &session.session_id,
            &started.turn_id,
            "context",
        )
        .unwrap();
    let replay = harness
        .host
        .steer_turn(
            "steer-replay",
            &session.session_id,
            &started.turn_id,
            "context",
        )
        .unwrap();
    assert_eq!(first.turn_id, replay.turn_id);
    assert_eq!(first.committed_position, replay.committed_position);
}

#[test]
fn steer_endpoint_returns_structured_command_response() {
    // End-to-end through the HTTP surface: POST a steer request, verify the
    // 200 body matches TurnCommandResponse shape (session_id, turn_id,
    // committed_position) — proves the route is wired and matches the
    // service-layer contract.
    use std::net::SocketAddr;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let harness = Harness::new(64);
        let session = harness
            .host
            .create_session("create-steer-http", "definition-main")
            .unwrap();
        let started = harness
            .host
            .start_turn("start-steer-http", &session.session_id, "first")
            .unwrap();
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let server = LiveHostServer::bind(harness.host.clone(), address)
            .await
            .unwrap();
        let listener_addr = server.local_addr();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server_handle = tokio::spawn(server.serve(async move {
            let _ = shutdown_rx.await;
        }));
        let url = format!(
            "http://{}/v1/turns/{}/events",
            listener_addr, started.turn_id
        );
        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Idempotency-Key", "steer-http-1")
            .header("Content-Type", "application/json")
            .body(format!(
                r#"{{"kind":"steer","session_id":"{}","text":"hello from steer"}}"#,
                session.session_id
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body_bytes = response.bytes().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["session_id"], session.session_id);
        assert_eq!(body["turn_id"], started.turn_id);
        assert!(body["committed_position"].as_u64().unwrap() > started.committed_position);
        let _ = shutdown_tx.send(());
        let _ = server_handle.await.unwrap();
    });
}

#[test]
fn h3_query_bounds_return_read_bound_exceeded_without_partial_views() {
    let harness = Harness::with_limits(
        64,
        Some(ActivityProjectionLimits {
            max_activities_per_turn: 8,
            max_activity_facts: 64,
            max_label_bytes: 128,
            max_activity_id_bytes: 128,
            max_encoded_bytes_per_turn: 1,
        }),
        HostReadLimits::PRODUCT_DEFAULT,
    );
    let session = harness
        .host
        .create_session("create-h3-bound", "definition-main")
        .unwrap();
    let started = harness
        .host
        .start_turn("start-h3-bound", &session.session_id, "hello")
        .unwrap();
    let session_id = SessionId::try_from(session.session_id.as_str()).unwrap();
    SqliteLedger::open(&harness.database)
        .unwrap()
        .commit(
            session_id,
            2,
            vec![FactDraft {
                fact_id: FactId::try_from("prepared-h3-bound").unwrap(),
                turn_id: Some(garive_ledger::TurnId::try_from(started.turn_id.as_str()).unwrap()),
                execution_id: Some(
                    garive_ledger::ExecutionId::try_from(started.execution_id.as_str()).unwrap(),
                ),
                model_request_id: None,
                tool_invocation_id: Some(ToolInvocationId::try_from("tool-h3-bound").unwrap()),
                kind: FactKind::new("effect.prepared").unwrap(),
                schema_version: 1,
                payload: CanonicalPayload::from_value(&serde_json::json!({
                    "prepared_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "tool_name":"private_reader_v9",
                    "tool_revision":"1",
                    "replay_class":"read_only",
                    "model_call_id":"call-h3-bound"
                }))
                .unwrap(),
                recorded_at: NOW.into(),
            }],
        )
        .unwrap();
    assert_eq!(
        harness.host.get_timeline(&session.session_id, 0, 4),
        Err(LiveHostError::ReadBoundExceeded)
    );
    assert_eq!(
        harness.host.read_event_page(&session.session_id, 0),
        Err(LiveHostError::ReadBoundExceeded)
    );
}

fn activity_catalogue() -> InstalledActivityCatalogue {
    InstalledActivityCatalogue {
        schema_version: 1,
        catalogue_revision: "activity-labels-1".into(),
        descriptors: vec![InstalledActivityDescriptor {
            tool_name: "private_reader_v9".into(),
            tool_revision: "1".into(),
            label_key: "agent.activity.read_file".into(),
        }],
    }
}

fn installed() -> InstalledAgent {
    installed_named(
        "definition-main",
        "revision-1",
        "installed-main",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    )
}

fn installed_named(
    definition_id: &str,
    definition_revision: &str,
    namespace: &str,
    snapshot_digest: &str,
) -> InstalledAgent {
    InstalledAgent {
        definition_id: definition_id.into(),
        definition_revision: definition_revision.into(),
        snapshot_digest: snapshot_digest.into(),
        agent_instance_namespace: namespace.into(),
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

#[test]
fn host_catalogue_binds_each_session_and_dispatch_to_one_exact_agent() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("catalogue.sqlite3");
    let dispatcher = Arc::new(VerifyingDispatcher {
        database: database.clone(),
        committed: Mutex::new(Vec::new()),
    });
    let alternate = installed_named(
        "definition-alternate",
        "revision-2",
        "installed-alternate",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let host = LiveHost::new_catalogue(
        &database,
        [installed(), alternate.clone()],
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(FixedClock),
        dispatcher.clone(),
    )
    .unwrap();

    assert_eq!(
        host.list_agent_definitions()
            .unwrap()
            .definitions
            .iter()
            .map(|value| value.definition_id.as_str())
            .collect::<Vec<_>>(),
        ["definition-alternate", "definition-main"]
    );
    let main = host
        .create_session("catalogue-main", "definition-main")
        .unwrap();
    let other = host
        .create_session("catalogue-other", "definition-alternate")
        .unwrap();
    host.start_turn("catalogue-main-turn", &main.session_id, "main")
        .unwrap();
    host.start_turn("catalogue-other-turn", &other.session_id, "other")
        .unwrap();
    let dispatched = dispatcher.committed.lock().unwrap();
    assert_eq!(dispatched[0].definition_id, "definition-main");
    assert_eq!(dispatched[0].definition_revision, "revision-1");
    assert_eq!(dispatched[1].definition_id, "definition-alternate");
    assert_eq!(dispatched[1].definition_revision, "revision-2");
    assert_eq!(dispatched[1].snapshot_digest, alternate.snapshot_digest);
    drop(dispatched);

    let restarted = LiveHost::new_catalogue(
        &database,
        [installed(), alternate],
        host.limits(),
        Arc::new(FixedClock),
        dispatcher,
    )
    .unwrap();
    assert_eq!(
        restarted
            .get_session(&other.session_id)
            .unwrap()
            .session
            .definition_id,
        "definition-alternate"
    );
    assert_eq!(
        restarted.create_session("missing", "definition-missing"),
        Err(LiveHostError::NotFound)
    );
}

#[test]
fn host_catalogue_rejects_empty_and_duplicate_definitions() {
    let directory = tempfile::tempdir().unwrap();
    let limits = LiveHostLimits {
        max_command_bytes: 4_096,
        event_batch_size: 64,
        event_poll_interval_ms: 10,
        activity: None,
    };
    let dispatcher = Arc::new(VerifyingDispatcher {
        database: directory.path().join("unused.sqlite3"),
        committed: Mutex::new(Vec::new()),
    });
    assert!(LiveHost::new_catalogue(
        directory.path().join("empty.sqlite3"),
        [],
        limits,
        Arc::new(FixedClock),
        dispatcher.clone(),
    )
    .is_err());
    assert!(LiveHost::new_catalogue(
        directory.path().join("duplicate.sqlite3"),
        [installed(), installed()],
        limits,
        Arc::new(FixedClock),
        dispatcher,
    )
    .is_err());
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

    let page = harness
        .host
        .get_timeline(&session.session_id, 0, 1)
        .unwrap();
    assert_eq!(page.api_version, "v1");
    assert_eq!(page.observed_max_position, 4);
    assert_eq!(page.scanned_through_position, 4);
    assert!(!page.has_more);
    assert_eq!(page.items[0].turn_id, first.turn_id);
    assert_eq!(page.items[0].user_text, "first");
    assert_eq!(page.items[0].state, "running");
    // Asking for a position beyond the watermark is rejected.
    assert_eq!(
        harness.host.get_timeline(&session.session_id, 5, 1),
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
fn public_session_surfaces_exclude_internal_planner_turns_and_content() {
    let harness = Harness::new(64);
    let installed = installed();
    let session = harness
        .host
        .create_session("planner-private", "definition-main")
        .unwrap();
    let session_id = SessionId::try_from(session.session_id.as_str()).unwrap();
    let planned = plan_start_plan_proposal_execution(
        &StartPlanProposalExecutionCommand {
            start: StartTurnCommand {
                command_id: RuntimeCommandId::new("planner-private-start").unwrap(),
                session_id: session_id.clone(),
                agent_instance_id: AgentInstanceId::try_from(session.agent_instance_id.as_str())
                    .unwrap(),
                definition_id: AgentDefinitionId::try_from(installed.definition_id.as_str())
                    .unwrap(),
                definition_revision: AgentDefinitionRevision::try_from(
                    installed.definition_revision.as_str(),
                )
                .unwrap(),
                snapshot_digest: installed.snapshot_digest,
                trusted_input: "planner-secret-request".into(),
                limits: installed.runtime_limits,
                recorded_at: NOW.into(),
            },
            goal_id: "goal-private".into(),
            goal_revision: 1,
            goal_definition_digest: "a".repeat(64),
            expected_session_version: 1,
            proposer_reference: "planner:test-v1".into(),
            output_schema_digest: "b".repeat(64),
        },
        1,
    )
    .unwrap();
    let mut ledger = SqliteLedger::open(&harness.database).unwrap();
    commit_planned_turn(&mut ledger, session_id.clone(), 1, &planned).unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        estimated: false,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: planned.turn_id.clone(),
            execution_id: planned.execution_id.clone().unwrap(),
            recorded_at: NOW.into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: "planner-secret-result".into(),
                }],
                usage,
            },
            completed_iterations: 1,
            usage,
        },
    )
    .unwrap();
    ledger.commit(session_id, 2, terminal).unwrap();
    drop(ledger);

    let public = harness
        .host
        .start_turn("ordinary-public", &session.session_id, "visible-user-input")
        .unwrap();
    let view = harness.host.get_session(&session.session_id).unwrap();
    assert_eq!(view.session.turn_count, 1);
    assert_eq!(
        view.session.latest_turn_id.as_deref(),
        Some(public.turn_id.as_str())
    );
    let timeline = harness
        .host
        .get_timeline(&session.session_id, 0, 8)
        .unwrap();
    assert_eq!(timeline.items.len(), 1);
    assert_eq!(timeline.items[0].turn_id, public.turn_id);
    let events = harness
        .host
        .read_event_page(&session.session_id, 0)
        .unwrap();
    assert!(events
        .events
        .iter()
        .all(|event| event.turn_id != planned.turn_id.as_str()));
    let encoded = format!(
        "{} {:?}",
        serde_json::to_string(&(view, timeline)).unwrap(),
        events.events
    );
    assert!(!encoded.contains("planner-secret"), "{encoded}");
    assert!(!encoded.contains(planned.turn_id.as_str()), "{encoded}");
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
    let timeline = harness
        .host
        .get_timeline(&session.session_id, 0, 10)
        .unwrap();
    assert_eq!(timeline.items[0].state, "running");
    assert!(timeline.items[0].cancellation_requested);
    assert_eq!(
        timeline.items[0].latest_position,
        cancelled.committed_position
    );
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

    let restarted = LiveHost::new(
        &harness.database,
        installed(),
        harness.host.limits(),
        Arc::new(FixedClock),
        harness.dispatcher.clone(),
    )
    .unwrap();
    assert!(
        restarted
            .get_timeline(&session.session_id, 0, 10)
            .unwrap()
            .items[0]
            .cancellation_requested
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
            serde_json::json!({"prepared_digest":digest,"tool_name":"private_reader_v9","tool_revision":"1","replay_class":"read_only","model_call_id":"call-h3"}),
        ),
        (
            "h3-authorized",
            "effect.authorized",
            serde_json::json!({"prepared_digest":digest,"grant_id":"grant-h3","authority_revision":"policy-1","granted_requirements":binding(serde_json::json!({}))}),
        ),
        (
            "h3-started",
            "effect.started",
            serde_json::json!({"prepared_digest":digest,"grant_id":"grant-h3","executor_id":"executor-private","executor_revision":"1","dispatch_attempt_id":"dispatch-h3"}),
        ),
        (
            "h3-receipt",
            "effect.receipt",
            serde_json::json!({"receipt_id":"receipt-h3","prepared_digest":digest,"grant_id":"grant-h3","executor_id":"executor-private","executor_revision":"1","classification":"completed","result_or_evidence":binding(serde_json::json!({"secret":"secret-tool-result"}))}),
        ),
        (
            "h3-completed",
            "effect.completed",
            serde_json::json!({"prepared_digest":digest,"receipt_id":"receipt-h3","result":binding(serde_json::json!({"secret":"secret-tool-result"}))}),
        ),
        (
            "h3-observation",
            "effect.observation",
            serde_json::json!({"prepared_digest":digest,"model_call_id":"call-h3","observation":binding(serde_json::json!({"secret":"secret-observation"}))}),
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
        serde_json::to_value(&page.events).unwrap()
    );
    assert_eq!(
        serde_json::to_value(restarted.get_timeline(&session.session_id, 0, 8).unwrap()).unwrap(),
        serde_json::to_value(&timeline).unwrap()
    );
    let public = serde_json::to_string(&(&page.events, &timeline)).unwrap();
    for forbidden in [
        "private_reader_v9",
        "secret-tool-result",
        "secret-observation",
        "executor-private",
        "grant-h3",
        "receipt-h3",
        "dispatch-h3",
    ] {
        assert!(!public.contains(forbidden), "leaked H3 canary: {forbidden}");
    }
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
async fn agent_registry_http_persists_exact_metadata_and_lifecycle() {
    let harness = Harness::new(64);
    let working = harness._directory.path().join("agent-work");
    let knowledge = harness._directory.path().join("knowledge");
    fs::create_dir(&working).unwrap();
    fs::create_dir(&knowledge).unwrap();
    fs::write(working.join("AGENT.md"), "# Atlas\n").unwrap();
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
    let base = format!("http://{address}/v1/agents");

    let created = client
        .post(&base)
        .header("idempotency-key", "agent-create")
        .header("content-type", "application/json")
        .body(
            serde_json::json!({
                "agent_id":"atlas",
                "working_directory":working,
                "readonly_knowledge_directories":[],
                "writable_knowledge_directory":null
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), reqwest::StatusCode::OK);
    let created: Value = serde_json::from_slice(&created.bytes().await.unwrap()).unwrap();
    assert_eq!(created["status"], "inactive");

    let page = client
        .get(&base)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let page: Value = serde_json::from_slice(&page).unwrap();
    assert_eq!(page["agents"].as_array().unwrap().len(), 1);
    let view = client
        .get(format!("{base}/atlas"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let view: Value = serde_json::from_slice(&view).unwrap();
    assert_eq!(view["working_directory"], created["working_directory"]);

    let updated = client
        .patch(format!("{base}/atlas"))
        .header("idempotency-key", "agent-update")
        .header("content-type", "application/json")
        .body(
            serde_json::json!({
                "readonly_knowledge_directories":[knowledge],
                "writable_knowledge_directory":null
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), reqwest::StatusCode::OK);

    let forbidden = client
        .patch(format!("{base}/atlas"))
        .header("idempotency-key", "agent-forbidden")
        .header("content-type", "application/json")
        .body(
            serde_json::json!({
                "working_directory":working,
                "readonly_knowledge_directories":[],
                "writable_knowledge_directory":null
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden.status(), reqwest::StatusCode::BAD_REQUEST);

    let inactive_session = client
        .post(format!("http://{address}/v1/sessions"))
        .header("idempotency-key", "inactive-session")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"atlas"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(
        inactive_session.status(),
        reqwest::StatusCode::PRECONDITION_FAILED
    );

    let active = client
        .post(format!("{base}/atlas/activate"))
        .header("idempotency-key", "agent-activate")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let active: Value = serde_json::from_slice(&active).unwrap();
    assert_eq!(active["status"], "active");
    let session = client
        .post(format!("http://{address}/v1/sessions"))
        .header("idempotency-key", "active-session")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"atlas"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(session.status(), reqwest::StatusCode::OK);
    let session: Value = serde_json::from_slice(&session.bytes().await.unwrap()).unwrap();
    let session_view = client
        .get(format!(
            "http://{address}/v1/sessions/{}",
            session["session_id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    let session_view: Value = serde_json::from_slice(&session_view.bytes().await.unwrap()).unwrap();
    assert_eq!(session_view["session"]["agent_id"], "atlas");
    fs::remove_file(working.join("AGENT.md")).unwrap();
    let invalid_run = client
        .post(format!(
            "http://{address}/v1/sessions/{}/turns",
            session["session_id"].as_str().unwrap()
        ))
        .header("idempotency-key", "invalid-agent-run")
        .header("content-type", "application/json")
        .body(r#"{"text":"must not run","delivery":"direct","agent_id":"atlas"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(
        invalid_run.status(),
        reqwest::StatusCode::PRECONDITION_FAILED
    );
    fs::write(working.join("AGENT.md"), "# Atlas restored\n").unwrap();
    let archived = client
        .post(format!("{base}/atlas/archive"))
        .header("idempotency-key", "agent-archive")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let archived: Value = serde_json::from_slice(&archived).unwrap();
    assert_eq!(archived["status"], "archived");
    let archived_session = client
        .post(format!("http://{address}/v1/sessions"))
        .header("idempotency-key", "archived-session")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"atlas"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(
        archived_session.status(),
        reqwest::StatusCode::PRECONDITION_FAILED
    );

    let _ = shutdown_tx.send(());
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn session_membership_is_metadata_and_replays_join_remove_rejoin() {
    let harness = Harness::new(64);
    let session = harness
        .host
        .create_session("create-membership", "definition-main")
        .unwrap();
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
    let base = format!("http://{address}/v1/sessions/{}/agents", session.session_id);

    let joined = client
        .post(&base)
        .header("idempotency-key", "join-future")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"future-agent"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(joined.status(), reqwest::StatusCode::OK);
    let joined: Value = serde_json::from_slice(&joined.bytes().await.unwrap()).unwrap();
    assert_eq!(joined["members"].as_array().unwrap().len(), 2);
    assert_eq!(joined["members"][1]["agent_id"], "future-agent");
    assert!(joined["members"][1].get("definition_id").is_none());
    let first_join_position = joined["members"][1]["joined_position"].as_u64().unwrap();

    let replay = client
        .post(&base)
        .header("idempotency-key", "join-future")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"future-agent"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), reqwest::StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&replay.bytes().await.unwrap()).unwrap(),
        joined
    );

    let removed = client
        .delete(format!("{base}/future-agent"))
        .header("idempotency-key", "remove-future")
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), reqwest::StatusCode::OK);
    let removed: Value = serde_json::from_slice(&removed.bytes().await.unwrap()).unwrap();
    assert_eq!(removed["members"].as_array().unwrap().len(), 1);

    let rejoined = client
        .post(&base)
        .header("idempotency-key", "rejoin-future")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"future-agent"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(rejoined.status(), reqwest::StatusCode::OK);
    let rejoined: Value = serde_json::from_slice(&rejoined.bytes().await.unwrap()).unwrap();
    assert!(rejoined["members"][1]["joined_position"].as_u64().unwrap() > first_join_position);

    let unresolved_turn = client
        .post(format!(
            "http://{address}/v1/sessions/{}/turns",
            session.session_id
        ))
        .header("idempotency-key", "turn-future")
        .header("content-type", "application/json")
        .body(r#"{"text":"hello","delivery":"direct","agent_id":"future-agent"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(unresolved_turn.status(), reqwest::StatusCode::NOT_FOUND);

    let working = harness._directory.path().join("active-member");
    fs::create_dir(&working).unwrap();
    fs::write(working.join("AGENT.md"), "# Active member\n").unwrap();
    harness
        .host
        .create_agent(
            "create-active-member",
            &garive_runtime::CreateAgentRequest {
                agent_id: "active-member".into(),
                working_directory: working,
                readonly_knowledge_directories: Vec::new(),
                writable_knowledge_directory: None,
            },
        )
        .unwrap();
    harness
        .host
        .activate_agent("activate-member", "active-member")
        .unwrap();
    let active_join = client
        .post(&base)
        .header("idempotency-key", "join-active")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"active-member"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(active_join.status(), reqwest::StatusCode::OK);
    let direct_turn = client
        .post(format!(
            "http://{address}/v1/sessions/{}/turns",
            session.session_id
        ))
        .header("idempotency-key", "turn-active")
        .header("content-type", "application/json")
        .body(r#"{"text":"hello","delivery":"direct","agent_id":"active-member"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(direct_turn.status(), reqwest::StatusCode::OK);
    let direct_turn: Value = serde_json::from_slice(&direct_turn.bytes().await.unwrap()).unwrap();
    assert_eq!(direct_turn["delivery"], "direct");
    assert_eq!(direct_turn["turns"].as_array().unwrap().len(), 1);
    assert_eq!(direct_turn["turns"][0]["agent_id"], "active-member");
    let removed_while_running = client
        .delete(format!("{base}/active-member"))
        .header("idempotency-key", "remove-active")
        .send()
        .await
        .unwrap();
    assert_eq!(removed_while_running.status(), reqwest::StatusCode::OK);
    let removed_turn = client
        .post(format!(
            "http://{address}/v1/sessions/{}/turns",
            session.session_id
        ))
        .header("idempotency-key", "turn-removed")
        .header("content-type", "application/json")
        .body(r#"{"text":"hello","delivery":"direct","agent_id":"active-member"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(
        removed_turn.status(),
        reqwest::StatusCode::PRECONDITION_FAILED
    );

    let conflict = client
        .post(&base)
        .header("idempotency-key", "rejoin-future")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"another-agent"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);

    let _ = shutdown_tx.send(());
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn explicit_broadcast_resolves_the_whole_roster_and_commits_atomically() {
    let harness = Harness::new(64);
    for agent_id in ["broadcast-alpha", "broadcast-beta"] {
        let working = harness._directory.path().join(agent_id);
        fs::create_dir(&working).unwrap();
        fs::write(working.join("AGENT.md"), format!("# {agent_id}\n")).unwrap();
        harness
            .host
            .create_agent(
                &format!("create-{agent_id}"),
                &garive_runtime::CreateAgentRequest {
                    agent_id: agent_id.into(),
                    working_directory: working,
                    readonly_knowledge_directories: Vec::new(),
                    writable_knowledge_directory: None,
                },
            )
            .unwrap();
        harness
            .host
            .activate_agent(&format!("activate-{agent_id}"), agent_id)
            .unwrap();
    }
    let session = harness
        .host
        .create_session("create-broadcast", "definition-main")
        .unwrap();
    let invalid_session = harness
        .host
        .create_session("create-invalid-broadcast", "definition-main")
        .unwrap();
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

    for (target, prefix) in [
        (&session.session_id, "broadcast"),
        (&invalid_session.session_id, "invalid"),
    ] {
        let base = format!("http://{address}/v1/sessions/{target}/agents");
        let removed = client
            .delete(format!("{base}/definition-main"))
            .header("idempotency-key", format!("{prefix}-remove-founder"))
            .send()
            .await
            .unwrap();
        assert_eq!(removed.status(), reqwest::StatusCode::OK);
        for agent_id in ["broadcast-alpha", "broadcast-beta"] {
            let joined = client
                .post(&base)
                .header("idempotency-key", format!("{prefix}-join-{agent_id}"))
                .header("content-type", "application/json")
                .body(format!(r#"{{"agent_id":"{agent_id}"}}"#))
                .send()
                .await
                .unwrap();
            assert_eq!(joined.status(), reqwest::StatusCode::OK);
        }
    }
    let invalid_base = format!(
        "http://{address}/v1/sessions/{}/agents",
        invalid_session.session_id
    );
    let missing = client
        .post(&invalid_base)
        .header("idempotency-key", "invalid-join-missing")
        .header("content-type", "application/json")
        .body(r#"{"agent_id":"missing-agent"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::OK);

    let turns = format!("http://{address}/v1/sessions/{}/turns", session.session_id);
    let response = client
        .post(&turns)
        .header("idempotency-key", "broadcast-turn")
        .header("content-type", "application/json")
        .body(r#"{"text":"hello all","delivery":"broadcast"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let response: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(response["delivery"], "broadcast");
    assert_eq!(response["turns"].as_array().unwrap().len(), 2);
    assert_eq!(response["turns"][0]["agent_id"], "broadcast-alpha");
    assert_eq!(response["turns"][1]["agent_id"], "broadcast-beta");
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 2);

    let replay: Value = serde_json::from_slice(
        &client
            .post(&turns)
            .header("idempotency-key", "broadcast-turn")
            .header("content-type", "application/json")
            .body(r#"{"text":"hello all","delivery":"broadcast"}"#)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay, response);
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 2);

    let invalid = client
        .post(format!(
            "http://{address}/v1/sessions/{}/turns",
            invalid_session.session_id
        ))
        .header("idempotency-key", "invalid-broadcast-turn")
        .header("content-type", "application/json")
        .body(r#"{"text":"must be atomic","delivery":"broadcast"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(harness.dispatcher.committed.lock().unwrap().len(), 2);

    for (key, body) in [
        ("invalid-shape-absent", r#"{"text":"ambiguous"}"#),
        (
            "invalid-shape-mixed",
            r#"{"text":"mixed","delivery":"broadcast","agent_id":"broadcast-alpha"}"#,
        ),
    ] {
        let ambiguous = client
            .post(&turns)
            .header("idempotency-key", key)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(ambiguous.status(), reqwest::StatusCode::BAD_REQUEST);
    }

    let _ = shutdown_tx.send(());
    task.await.unwrap().unwrap();
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
            .post(format!("{base}/v1/turns/turn-x/continue"))
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
    let goals = client
        .get(format!("{base}/v1/sessions/{session_id}/goals"))
        .send()
        .await
        .unwrap();
    assert_eq!(goals.status(), reqwest::StatusCode::OK);
    let goals: Value = serde_json::from_slice(&goals.bytes().await.unwrap()).unwrap();
    assert_eq!(goals["api_version"], "v1");
    assert_eq!(goals["session_id"], session_id);
    assert_eq!(goals["session_version"], 1);
    assert!(goals["goals"].as_array().unwrap().is_empty());
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
        .body(r#"{"text":"hello","delivery":"direct","agent_id":"definition-main"}"#)
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

    let wake_snapshot = client
        .get(format!("{base}/internal/mobile/wake-snapshot?limit=20"))
        .send()
        .await
        .unwrap();
    assert_eq!(wake_snapshot.status(), reqwest::StatusCode::OK);
    let wake_snapshot: Value =
        serde_json::from_slice(&wake_snapshot.bytes().await.unwrap()).unwrap();
    assert_eq!(wake_snapshot["api_version"], "v1");
    assert_eq!(wake_snapshot["observations"][0]["session_id"], session_id);
    assert_eq!(
        wake_snapshot["observations"][0]["latest_position"],
        timeline["observed_max_position"]
    );
    assert!(wake_snapshot["observations"][0]
        .get("wake_category")
        .is_none());

    let bad_timeline = client
        .get(format!(
            "{base}/v1/sessions/{session_id}/timeline?limit=20&unknown=1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_timeline.status(), reqwest::StatusCode::BAD_REQUEST);

    let bad_wake = client
        .get(format!(
            "{base}/internal/mobile/wake-snapshot?limit=20&unknown=1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_wake.status(), reqwest::StatusCode::BAD_REQUEST);

    let started_payload = started
        .bytes()
        .await
        .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap())
        .unwrap();
    let started_turn_id = started_payload["turns"][0]["turn_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = client
        .get(format!(
            "{base}/v1/turns/{started_turn_id}/events?after_position=0"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut bytes = response.bytes_stream();
    let first = bytes.next().await.unwrap().unwrap();
    let text = String::from_utf8(first.to_vec()).unwrap();
    assert!(text.contains("event: host"));
    assert!(text.contains("turn.started"));
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

fn goal_definition(
    goal_id: &str,
    objective: &str,
    parent: Option<&str>,
    session_id: &str,
) -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new(goal_id).unwrap(),
        objective,
        vec![GoalCriterion::UserAcceptance {
            criterion_id: GoalCriterionId::new("accepted").unwrap(),
            response_schema_digest: "a".repeat(64),
        }],
        GoalScopeV1::new(Some(session_id.into()), ["workspace-1".into()]).unwrap(),
        GoalBoundsV1::new(2, 3, 2, Some(10_000), Some(60_000)).unwrap(),
        parent.map(|value| GoalId::new(value).unwrap()),
        [GoalCapabilityReference::new("tools", "catalogue-v1").unwrap()],
    )
    .unwrap()
}

fn goal_context(command_id: &str) -> GoalCommandContext {
    GoalCommandContext {
        command_id: command_id.into(),
        actor_reference: "user:fixture".into(),
        recorded_at: NOW.into(),
    }
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
