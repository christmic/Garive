use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelFuture, ModelItem, ModelObserver, ModelPort,
    ModelRequest, ModelStopReason, ModelUsage, TokenCount, UsageSource,
};
use garive_multiagent::{CollaborationToolCatalogue, DELEGATE_TOOL, MESSAGE_AGENT_TOOL};
use garive_runtime::{
    headless::{
        build_headless_installation, headless_execution_attempt, headless_execution_policy,
        HeadlessConfiguration,
    },
    AutonomousCollaborationExecutor, AutonomousCollaborationOutbox,
    CatalogueBoundGovernedExecutionFactory, CatalogueCapabilityPreparationFactory, CommittedTurn,
    EffectiveRuntimeLimits, ExecutorDispatch, ExecutorPort, HeadlessWorkspaceExecutionFactory,
    HostClock, InstalledAgent, LiveHost, LiveHostLimits, LocalExecutionWorker,
    ManagementConfigState, SqliteLedger, TurnDispatchError, TurnDispatcher,
    COLLABORATION_POLICY_REVISION,
};
use garive_tools::{GrantId, InvocationGrant, ToolIntent, ToolInvocationId};
use tempfile::TempDir;

struct FixedClock;

impl HostClock for FixedClock {
    fn recorded_at(&self) -> String {
        "2026-09-03T00:00:00Z".into()
    }
}

struct RecordingDispatcher {
    turns: Mutex<Vec<CommittedTurn>>,
}

impl TurnDispatcher for RecordingDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        self.turns.lock().unwrap().push(turn.clone());
        Ok(())
    }
}

struct Harness {
    _directory: TempDir,
    database: PathBuf,
    dispatcher: Arc<RecordingDispatcher>,
    host: LiveHost,
}

impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("host.sqlite3");
        let dispatcher = Arc::new(RecordingDispatcher {
            turns: Mutex::new(Vec::new()),
        });
        let host = LiveHost::new(
            &database,
            InstalledAgent {
                definition_id: "definition-main".into(),
                definition_revision: "revision-1".into(),
                snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .into(),
                agent_instance_namespace: "autonomous-test".into(),
                public_capabilities: vec!["collaboration".into()],
                runtime_limits: EffectiveRuntimeLimits {
                    max_iterations: 4,
                    max_input_tokens: Some(1_024),
                    max_output_tokens: Some(512),
                    deadline_budget_ms: Some(30_000),
                },
                public_activity_catalogue: None,
            },
            LiveHostLimits {
                max_command_bytes: 65_536,
                event_batch_size: 64,
                event_poll_interval_ms: 10,
                activity: None,
            },
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

#[tokio::test]
async fn authenticated_agent_tool_publishes_without_model_supplied_sender() {
    let harness = Harness::new();
    let session = harness
        .host
        .create_named_session("session", "definition-main", "Atlas")
        .unwrap();
    let roster = harness
        .host
        .join_session_agent("join", &session.session_id, "definition-main", "Birch")
        .unwrap();
    harness
        .host
        .start_agent_turn(
            "atlas-turn",
            &session.session_id,
            &roster.members[0].agent_instance_id,
            "Send Birch a message",
        )
        .unwrap();
    let committed = harness.dispatcher.turns.lock().unwrap()[0].clone();
    let outbox = AutonomousCollaborationOutbox::default();
    let mut executor =
        AutonomousCollaborationExecutor::new(&harness.database, &committed, outbox.clone())
            .unwrap();
    let catalogue = CollaborationToolCatalogue::new(COLLABORATION_POLICY_REVISION).unwrap();
    let prepared = catalogue
        .prepare(&ToolIntent::new(
            "model-call",
            MESSAGE_AGENT_TOOL,
            r#"{"recipient":"Birch","text":"AUTONOMOUS_HELLO"}"#,
        ))
        .unwrap();
    let invocation = ToolInvocationId::new("invocation-autonomous-message").unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("grant-autonomous-message").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "constraints",
        COLLABORATION_POLICY_REVISION,
    )
    .unwrap();
    let execution = executor.prepare(&invocation, &prepared, &grant).unwrap();
    let terminal = executor
        .dispatch(ExecutorDispatch {
            invocation_id: &invocation,
            prepared: &prepared,
            grant: &grant,
            execution: &execution,
            receipt_id: "receipt-autonomous-message",
        })
        .await
        .unwrap();
    let receipt = match terminal {
        garive_tools::ExecutionFact::Completed {
            receipt: Some(receipt),
            ..
        } => receipt,
        _ => panic!("completed receipt required"),
    };
    executor.acknowledge_receipt(&invocation, &receipt).unwrap();
    let drained = outbox.drain(&harness.host);
    assert_eq!(drained.published, 1);
    assert_eq!(drained.retained, 0);
    let messages = harness
        .host
        .get_session_agent_messages(&session.session_id)
        .unwrap();
    assert_eq!(messages.messages.len(), 1);
    assert_eq!(
        messages.messages[0].from_agent_instance_id,
        roster.members[0].agent_instance_id
    );
    assert_eq!(
        messages.messages[0].to_agent_instance_id.as_deref(),
        Some(roster.members[1].agent_instance_id.as_str())
    );
    assert_eq!(messages.messages[0].text, "AUTONOMOUS_HELLO");
}

#[tokio::test]
async fn named_target_is_rejected_before_effect_dispatch() {
    let harness = Harness::new();
    let session = harness
        .host
        .create_named_session("session", "definition-main", "Atlas")
        .unwrap();
    harness
        .host
        .start_agent_turn(
            "atlas-turn",
            &session.session_id,
            &session.agent_instance_id,
            "Send a message",
        )
        .unwrap();
    let committed = harness.dispatcher.turns.lock().unwrap()[0].clone();
    let mut executor = AutonomousCollaborationExecutor::new(
        &harness.database,
        &committed,
        AutonomousCollaborationOutbox::default(),
    )
    .unwrap();
    let catalogue = CollaborationToolCatalogue::new(COLLABORATION_POLICY_REVISION).unwrap();
    let prepared = catalogue
        .prepare(&ToolIntent::new(
            "model-call",
            MESSAGE_AGENT_TOOL,
            r#"{"recipient":"Missing","text":"hello"}"#,
        ))
        .unwrap();
    let invocation = ToolInvocationId::new("invocation-missing-target").unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("grant-missing-target").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "constraints",
        COLLABORATION_POLICY_REVISION,
    )
    .unwrap();
    assert!(executor.prepare(&invocation, &prepared, &grant).is_err());
}

struct ToolCallingModel(AtomicUsize);

impl ModelPort for ToolCallingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let first = self.0.fetch_add(1, Ordering::SeqCst) == 0;
            if first {
                assert!(request
                    .tools
                    .iter()
                    .any(|tool| tool.name == MESSAGE_AGENT_TOOL));
            }
            Ok(InvokeOutcome::Completed {
                items: if first {
                    vec![ModelItem::ToolIntent {
                        model_call_id: "model-autonomous-message".into(),
                        tool_name: MESSAGE_AGENT_TOOL.into(),
                        arguments_json: r#"{"recipient":"Birch","text":"MODEL_ORIGINATED_HELLO"}"#
                            .into(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "MESSAGE_ACCEPTED".into(),
                    }]
                },
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: if first {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

struct DelegatingModel(AtomicUsize);

impl ModelPort for DelegatingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.0.fetch_add(1, Ordering::SeqCst);
            let (items, stop_reason) = match call {
                0 => (
                    vec![ModelItem::ToolIntent {
                        model_call_id: "model-autonomous-delegation".into(),
                        tool_name: DELEGATE_TOOL.into(),
                        arguments_json: r#"{"assignee":{"kind":"named","agent_name":"Birch"},"objective":"Return CHILD_RESULT"}"#.into(),
                    }],
                    ModelStopReason::ToolUse,
                ),
                1 => (
                    vec![ModelItem::Text {
                        text: "PARENT_CONTINUED".into(),
                    }],
                    ModelStopReason::EndTurn,
                ),
                _ => (
                    vec![ModelItem::Text {
                        text: "CHILD_RESULT".into(),
                    }],
                    ModelStopReason::EndTurn,
                ),
            };
            Ok(InvokeOutcome::Completed {
                items,
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason,
            })
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn model_tool_intent_is_bound_to_the_active_agent_and_published() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("host.sqlite3");
    let configuration = HeadlessConfiguration {
        state: ManagementConfigState {
            profile_id: "openai.responses.v1".into(),
            endpoint_override: None,
            model_target_id: "model".into(),
            model_id: "model".into(),
            deployment_id: "deployment".into(),
            definition_id: "desktop.agent.v3".into(),
            runtime_id: "runtime".into(),
            configuration_revision: 1,
            configuration_digest: "a".repeat(64),
            committed_at: "2026-09-03T00:00:00Z".into(),
        },
        api_key: "fixture-only".into(),
    };
    let (installation, catalogue) = build_headless_installation(&configuration).unwrap();
    let (host, _, mut queue) = LiveHost::new_with_worker(
        &database,
        [installation.clone_installed_agent()],
        LiveHostLimits {
            max_command_bytes: 65_536,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(FixedClock),
        16,
    )
    .unwrap();
    let session = host
        .create_named_session("model-session", "desktop.agent.v3", "Atlas")
        .unwrap();
    let roster = host
        .join_session_agent(
            "model-join",
            &session.session_id,
            "desktop.agent.v3",
            "Birch",
        )
        .unwrap();
    let outbox = AutonomousCollaborationOutbox::default();
    let governed = Arc::new(
        HeadlessWorkspaceExecutionFactory::collaboration_only(&database, outbox.clone()).unwrap(),
    );
    let worker = LocalExecutionWorker::new_governed(
        &database,
        headless_execution_policy(&configuration),
        Arc::new(ToolCallingModel(AtomicUsize::new(0))),
        Arc::new(CatalogueBoundGovernedExecutionFactory::new(
            catalogue.clone(),
            governed,
        )),
        Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None)),
    )
    .unwrap();
    host.start_agent_turn(
        "model-turn",
        &session.session_id,
        &roster.members[0].agent_instance_id,
        "Send Birch the requested marker yourself.",
    )
    .unwrap();
    let worker_result = queue
        .try_run_next(&worker, &headless_execution_attempt(1_000))
        .await;
    if worker_result.is_err() {
        let ledger = SqliteLedger::open(&database).unwrap();
        let session_id = garive_ledger::SessionId::try_from(session.session_id.as_str()).unwrap();
        let watermark = ledger.session_watermark(&session_id).unwrap().unwrap();
        let facts = ledger
            .read_facts(&session_id, 0, watermark.max_position, None)
            .unwrap();
        panic!(
            "worker failed: {worker_result:?}; facts={:?}",
            facts
                .iter()
                .map(|fact| (fact.position, fact.kind.as_str(), fact.payload.as_json()))
                .collect::<Vec<_>>()
        );
    }
    let recovered_outbox = AutonomousCollaborationOutbox::default();
    assert_eq!(recovered_outbox.recover(&database).unwrap(), 1);
    let drain = recovered_outbox.drain(&host);
    assert_eq!(drain.published, 1);
    assert_eq!(drain.retained, 0);
    let messages = host
        .get_session_agent_messages(&session.session_id)
        .unwrap();
    assert_eq!(messages.messages.len(), 1);
    assert_eq!(messages.messages[0].text, "MODEL_ORIGINATED_HELLO");
    assert_eq!(
        messages.messages[0].from_agent_instance_id,
        roster.members[0].agent_instance_id
    );
    assert_eq!(
        messages.messages[0].to_agent_instance_id.as_deref(),
        Some(roster.members[1].agent_instance_id.as_str())
    );

    let ledger = SqliteLedger::open(&database).unwrap();
    let session_id = garive_ledger::SessionId::try_from(session.session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session_id).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session_id, 0, watermark.max_position, None)
        .unwrap();
    assert!(facts.iter().any(|fact| {
        fact.kind.as_str() == "effect.prepared"
            && serde_json::from_str::<serde_json::Value>(fact.payload.as_json()).is_ok_and(
                |value| {
                    value.get("tool_name").and_then(serde_json::Value::as_str)
                        == Some(MESSAGE_AGENT_TOOL)
                },
            )
    }));
    assert!(facts.iter().any(|fact| {
        fact.kind.as_str() == "safety.decided"
            && serde_json::from_str::<serde_json::Value>(fact.payload.as_json()).is_ok_and(
                |value| {
                    value
                        .get("actor_authority_reference")
                        .and_then(serde_json::Value::as_str)
                        == Some(format!("agent:{}", roster.members[0].agent_instance_id).as_str())
                },
            )
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn model_delegation_runs_child_without_suspending_parent_and_delivers_result() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("host.sqlite3");
    let configuration = HeadlessConfiguration {
        state: ManagementConfigState {
            profile_id: "openai.responses.v1".into(),
            endpoint_override: None,
            model_target_id: "model".into(),
            model_id: "model".into(),
            deployment_id: "deployment".into(),
            definition_id: "desktop.agent.v3".into(),
            runtime_id: "runtime".into(),
            configuration_revision: 1,
            configuration_digest: "a".repeat(64),
            committed_at: "2026-09-03T00:00:00Z".into(),
        },
        api_key: "fixture-only".into(),
    };
    let (installation, catalogue) = build_headless_installation(&configuration).unwrap();
    let (host, _, mut queue) = LiveHost::new_with_worker(
        &database,
        [installation.clone_installed_agent()],
        LiveHostLimits {
            max_command_bytes: 65_536,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(FixedClock),
        16,
    )
    .unwrap();
    let session = host
        .create_named_session("delegate-session", "desktop.agent.v3", "Atlas")
        .unwrap();
    let roster = host
        .join_session_agent(
            "delegate-join",
            &session.session_id,
            "desktop.agent.v3",
            "Birch",
        )
        .unwrap();
    let outbox = AutonomousCollaborationOutbox::default();
    let governed = Arc::new(
        HeadlessWorkspaceExecutionFactory::collaboration_only(&database, outbox.clone()).unwrap(),
    );
    let worker = LocalExecutionWorker::new_governed(
        &database,
        headless_execution_policy(&configuration),
        Arc::new(DelegatingModel(AtomicUsize::new(0))),
        Arc::new(CatalogueBoundGovernedExecutionFactory::new(
            catalogue.clone(),
            governed,
        )),
        Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None)),
    )
    .unwrap();
    let parent = host
        .start_agent_turn(
            "delegate-turn",
            &session.session_id,
            &roster.members[0].agent_instance_id,
            "Delegate to Birch and keep going.",
        )
        .unwrap();
    queue
        .try_run_next(&worker, &headless_execution_attempt(2_000))
        .await
        .unwrap();
    let ledger = SqliteLedger::open(&database).unwrap();
    let session_id = garive_ledger::SessionId::try_from(session.session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session_id).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session_id, 0, watermark.max_position, None)
        .unwrap();
    assert!(facts.iter().any(|fact| {
        fact.kind.as_str() == "turn.completed"
            && fact.turn_id.as_ref().map(|turn| turn.as_str()) == Some(parent.turn_id.as_str())
    }));

    let drain = outbox.drain(&host);
    let [(delegation_session, delegation_id)] = drain.delegation_ids.as_slice() else {
        panic!("one autonomous delegation required")
    };
    queue
        .try_run_next(&worker, &headless_execution_attempt(3_000))
        .await
        .unwrap();
    assert!(host
        .deliver_agent_task_result(delegation_session, delegation_id)
        .unwrap());
    let messages = host
        .get_session_agent_messages(&session.session_id)
        .unwrap();
    assert_eq!(messages.messages.last().unwrap().text, "CHILD_RESULT");
    assert_eq!(
        messages.messages.last().unwrap().from_agent_instance_id,
        roster.members[1].agent_instance_id
    );
    assert_eq!(
        messages
            .messages
            .last()
            .unwrap()
            .to_agent_instance_id
            .as_deref(),
        Some(roster.members[0].agent_instance_id.as_str())
    );
}
