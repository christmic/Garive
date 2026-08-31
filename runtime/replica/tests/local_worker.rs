use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
    ToolPreparationPort,
};
use garive_ledger::{CanonicalPayload, ExecutionId, SessionId, TurnId};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelOutputKind, ModelOutputSettings, ModelPort, ModelRequest, ModelStopReason,
    ModelStreamEvent, ModelUsage, ObserverDecision, TextMode, TokenCount, UsageSource,
};
use garive_runtime::{
    local_dispatch_queue, recover_local_dispatches, CommittedTurn, EffectiveRuntimeLimits,
    HostClock, InstalledAgent, LiveHost, LiveHostLimits, LiveOutputEndReason, LiveOutputEventKind,
    LiveOutputHub, LiveOutputLimits, LocalCapabilityPreparationFactory,
    LocalCapabilityPreparationInput, LocalExecutionAttempt, LocalExecutionPolicy,
    LocalExecutionWorker, LocalF0Governance, LocalGovernedExecution, LocalGovernedExecutionFactory,
    LocalWorkerDisposition, LocalWorkerError, PreparedAgentCapabilities, SqliteLedger,
    TurnDispatcher,
};
use garive_runtime::{
    AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest, ExecutorDispatch,
    ExecutorFuture, ExecutorPort, F0GovernanceContext, PreparedExecution, SafetyDecisionV1,
    SafetyDisposition, SafetyEvaluation, SafetyFuture, SafetyPort, SandboxAdmission,
    SandboxAdmissionPort, SandboxAdmissionRequest, SandboxBindingV1,
};
use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, EffectReceipt, ExecutionCapability,
    ExecutionFact, ExecutionRequirements, InvocationAccessSet, PreparationError, PreparedToolCall,
    ReceiptId, ReplayClass, ResourceAccess, SandboxControl, SandboxRequirementsV1,
    TerminalClassification, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition,
    ToolIntent,
};
use serde_json::json;
use tempfile::tempdir;

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-08-29T00:00:00Z".into()
    }
}

struct CompletingModel(AtomicUsize);
impl ModelPort for CompletingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert_eq!(request.target_id.as_str(), "target-main");
            assert_eq!(
                observer.observe(&ModelStreamEvent::OutputItemStarted {
                    output_index: 0,
                    kind: ModelOutputKind::Text,
                }),
                ObserverDecision::Continue
            );
            assert_eq!(
                observer.observe(&ModelStreamEvent::TextDelta {
                    output_index: 0,
                    delta: "durable ".into(),
                }),
                ObserverDecision::Continue
            );
            assert_eq!(
                observer.observe(&ModelStreamEvent::TextDelta {
                    output_index: 0,
                    delta: "answer".into(),
                }),
                ObserverDecision::Continue
            );
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "durable answer".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(2),
                    output_tokens: TokenCount::Known(3),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

struct EmptyCapabilityPreparation(AtomicUsize);
impl LocalCapabilityPreparationFactory for EmptyCapabilityPreparation {
    fn prepare(
        &self,
        ledger: &SqliteLedger,
        input: LocalCapabilityPreparationInput<'_>,
    ) -> Result<PreparedAgentCapabilities, LocalWorkerError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        assert_eq!(input.recorded_at, "2026-08-29T00:00:01Z");
        assert_eq!(
            input.request.session_id.as_str(),
            input.committed.session_id.as_str()
        );
        assert!(ledger.load_turn(&input.committed.turn_id).is_ok());
        Ok(PreparedAgentCapabilities::default())
    }
}

struct ToolThenTextModel(AtomicUsize);
impl ModelPort for ToolThenTextModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let tool = self.0.fetch_add(1, Ordering::SeqCst) == 0;
            Ok(InvokeOutcome::Completed {
                items: if tool {
                    vec![ModelItem::ToolIntent {
                        model_call_id: "call-write".into(),
                        tool_name: "write_file".into(),
                        arguments_json: r#"{"path":"result.md"}"#.into(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "artifact committed".into(),
                    }]
                },
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(2),
                    output_tokens: TokenCount::Known(3),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: if tool {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

struct Approve;
impl AuthorityPort for Approve {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            Ok(AuthorityDecision::Approve {
                granted_requirements: request.prepared.requirements().clone(),
                constraints_digest: "b".repeat(64),
                authority_revision: "desktop-test-1".into(),
            })
        })
    }
}

struct CompleteEffect;
impl ExecutorPort for CompleteEffect {
    fn prepare(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        _: &garive_tools::PreparedToolCall,
        _: &garive_tools::InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        Ok(PreparedExecution {
            executor_id: "desktop.workspace".into(),
            executor_revision: "1".into(),
            dispatch_attempt_id: "dispatch-write-1".into(),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        Box::pin(async move {
            let content = json!({"artifact_id":"artifact-1"});
            Ok(ExecutionFact::Completed {
                receipt: Some(EffectReceipt {
                    receipt_id: ReceiptId::new(command.receipt_id).unwrap(),
                    invocation_id: command.invocation_id.clone(),
                    prepared_digest: command.prepared.input_digest().into(),
                    grant_id: command.grant.grant_id.clone(),
                    executor_id: command.execution.executor_id.clone(),
                    executor_revision: command.execution.executor_revision.clone(),
                    terminal_classification: TerminalClassification::Completed,
                    result_digest: CanonicalPayload::from_value(&content)
                        .unwrap()
                        .sha256()
                        .into(),
                }),
                content,
                truncated: false,
            })
        })
    }
}

struct GovernedFactory;

struct WriteResolver;
impl ToolAccessResolver for WriteResolver {
    fn revision(&self) -> &str {
        "t1-write-resolver-1"
    }
    fn resolve(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<InvocationAccessSet, PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap(),
            AccessMode::Write,
        )?])
    }
}

struct WritePreparation(ToolCatalog);
impl ToolPreparationPort for WritePreparation {
    fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.0.prepare_v3(intent, &WriteResolver)
    }
}

struct NoRecoveryContent;
impl garive_runtime::F0RecoveryContentPort for NoRecoveryContent {
    fn resolve(&mut self, _: &str) -> Result<String, garive_runtime::F0RecoveryError> {
        Err(garive_runtime::F0RecoveryError::ContentUnavailable)
    }
}

struct AllowF0;
impl SafetyPort for AllowF0 {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async move {
            Ok(SafetyEvaluation {
                decision: SafetyDecisionV1::new(
                    "local-safety",
                    SafetyDisposition::Allow,
                    request.invocation_id().clone(),
                    request.prepared_digest(),
                    Some("b".repeat(64)),
                    "desktop-test-1",
                    None,
                )
                .unwrap(),
                granted_requirements: Some(
                    ExecutionRequirements::new(
                        [ExecutionCapability::FilesystemWrite],
                        1_000,
                        4_096,
                    )
                    .unwrap(),
                ),
                interaction: None,
            })
        })
    }
}

struct LocalF0Sandbox;
impl SandboxAdmissionPort for LocalF0Sandbox {
    fn admit(
        &mut self,
        _: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, garive_runtime::GovernedRuntimePortError> {
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                "local-binding",
                "workspace",
                "desktop.workspace",
                "1",
                "desktop-test-1",
                write_policy(),
                write_sandbox(),
            )
            .unwrap(),
            effective_limits_digest: "e".repeat(64),
            preflight_id: "local-preflight".into(),
            dispatch_attempt_id: "dispatch-write-1".into(),
        })
    }
}

fn write_policy() -> ToolAccessPolicyV1 {
    ToolAccessPolicyV1::new(
        "t1-write-policy-1",
        [AccessPolicyEntry::new("result.md", [AccessMode::Write]).unwrap()],
        [],
        [],
        [],
        1,
        4_096,
    )
    .unwrap()
}

fn write_sandbox() -> SandboxRequirementsV1 {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemWrite],
        [
            SandboxControl::FilesystemScope,
            SandboxControl::SymlinkContainment,
            SandboxControl::ResourceLimits,
        ],
        None,
        8,
    )
    .unwrap()
}

fn governed_definition() -> ToolDefinition {
    ToolDefinition::new_v3(
        "write_file", "1", "Write one governed Workspace artifact.",
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        ExecutionRequirements::new([ExecutionCapability::FilesystemWrite], 1_000, 4_096).unwrap(),
        ReplayClass::ReceiptRecoverable, write_policy(), "t1-write-resolver-1", write_sandbox(),
    ).unwrap()
}

impl LocalGovernedExecutionFactory for GovernedFactory {
    fn create(&self, _: &CommittedTurn) -> Result<LocalGovernedExecution, LocalWorkerError> {
        Ok(LocalGovernedExecution {
            capabilities: garive_core::AgentToolCapabilities {
                definitions: vec![governed_definition()],
            },
            authority: Box::new(Approve),
            executor: Box::new(CompleteEffect),
            f0: LocalF0Governance {
                preparation: Box::new(WritePreparation(
                    ToolCatalog::new([governed_definition()]).unwrap(),
                )),
                recovery_content: Box::new(NoRecoveryContent),
                safety: Box::new(AllowF0),
                sandbox: Box::new(LocalF0Sandbox),
                context: F0GovernanceContext {
                    actor_authority_reference: "actor:test".into(),
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: "desktop-test-1".into(),
                },
            },
        })
    }
}

fn policy() -> LocalExecutionPolicy {
    LocalExecutionPolicy {
        model_target_id: "target-main".into(),
        deployment_id: "deployment-main".into(),
        recovery_policy_revision: "recovery-1".into(),
        required_capabilities: vec![ModelCapability::Text],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 0,
            output_limit: OutputLimitAction::Suspend,
            transport: TerminalRecoveryAction::Suspend,
            unavailable: TerminalRecoveryAction::Suspend,
            missing_usage: MissingUsagePolicy::Stop,
        },
        max_context_items: 8,
        max_context_utf8_bytes: 1_024,
        max_model_attempts: 1,
    }
}

fn attempt() -> LocalExecutionAttempt {
    LocalExecutionAttempt {
        worker_owner_id: "worker-1".into(),
        lease_token: "unpredictable-test-token".into(),
        now_ms: 1_000,
        lease_duration_ms: 5_000,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

#[tokio::test]
async fn committed_turn_runs_to_durable_host_terminal_once() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let (dispatcher, mut queue) = local_dispatch_queue(1).expect("queue");
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: Some(1_000),
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        dispatcher,
    )
    .expect("host");
    let session = host
        .create_session("create-1", "definition-main")
        .expect("session");
    let turn = host
        .start_turn("start-1", &session.session_id, "hello")
        .expect("turn start committed");

    let live_output = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 1,
        max_preview_bytes: 1_024,
        max_event_bytes: 64,
        broadcast_capacity: 16,
        max_subscribers_per_session: 2,
    })
    .expect("live output");
    let mut live = live_output
        .subscribe(&session.session_id)
        .expect("subscriber");
    let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
    let capability_preparation = Arc::new(EmptyCapabilityPreparation(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new(&database, policy(), model.clone())
        .expect("worker")
        .with_capability_preparation(capability_preparation.clone())
        .with_live_output(live_output);
    let disposition = queue
        .try_run_next(&worker, &attempt())
        .await
        .expect("worker terminal");
    let terminal_positions = match disposition {
        LocalWorkerDisposition::TerminalCommitted { positions } => positions,
        LocalWorkerDisposition::AlreadyTerminal => panic!("first dispatch was terminal"),
    };
    assert_eq!(terminal_positions.len(), 2);
    assert!(terminal_positions[0] > turn.committed_position);
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    assert_eq!(capability_preparation.0.load(Ordering::SeqCst), 1);
    let mut live_events = Vec::new();
    while let Some(event) = live.try_recv().expect("live receive") {
        live_events.push(event);
    }
    assert!(live_events.iter().any(|event| matches!(
        &event.kind,
        LiveOutputEventKind::TextDelta { text } if text == "durable "
    )));
    assert!(live_events.iter().any(|event| matches!(
        &event.kind,
        LiveOutputEventKind::TextDelta { text } if text == "answer"
    )));
    assert!(matches!(
        live_events.last().map(|event| &event.kind),
        Some(LiveOutputEventKind::Ended {
            reason: LiveOutputEndReason::TerminalCommitted
        })
    ));
    let duplicate = CommittedTurn {
        session_id: SessionId::try_from(session.session_id.as_str()).expect("session identity"),
        turn_id: TurnId::try_from(turn.turn_id.as_str()).expect("turn identity"),
        execution_id: ExecutionId::try_from(turn.execution_id.as_str())
            .expect("execution identity"),
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "a".repeat(64),
        session_version: 2,
        committed_position: turn.committed_position,
    };
    assert_eq!(
        worker.execute(&duplicate, &attempt()).await,
        Ok(LocalWorkerDisposition::AlreadyTerminal)
    );
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    assert_eq!(capability_preparation.0.load(Ordering::SeqCst), 1);

    let page = host
        .read_event_page(&session.session_id, 0)
        .expect("event page");
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.event.as_str())
            .collect::<Vec<_>>(),
        ["session.created", "turn.started", "turn.completed"]
    );
    assert_eq!(
        page.events.last().expect("terminal event").text,
        "durable answer"
    );
    assert!(page.events[2].position > page.events[1].position + 1);
    assert_eq!(
        queue.try_run_next(&worker, &attempt()).await,
        Err(LocalWorkerError::QueueEmpty)
    );
}

#[tokio::test]
async fn explicit_governed_factory_runs_the_complete_f0_effect_fact_chain() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("governed.db");
    let (dispatcher, mut queue) = local_dispatch_queue(1).expect("queue");
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: Some(1_000),
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        dispatcher,
    )
    .expect("host");
    let session = host
        .create_session("create-governed", "definition-main")
        .expect("session");
    let turn = host
        .start_turn("start-governed", &session.session_id, "create artifact")
        .expect("turn");
    let mut governed_policy = policy();
    governed_policy.required_capabilities = vec![ModelCapability::Text, ModelCapability::Tools];
    let model = Arc::new(ToolThenTextModel(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new_governed(
        &database,
        governed_policy,
        model.clone(),
        Arc::new(GovernedFactory),
    )
    .expect("governed worker");
    assert!(matches!(
        queue.try_run_next(&worker, &attempt()).await,
        Ok(LocalWorkerDisposition::TerminalCommitted { .. })
    ));
    assert_eq!(model.0.load(Ordering::SeqCst), 2);
    let snapshot = SqliteLedger::open(&database)
        .unwrap()
        .load_turn(&TurnId::try_from(turn.turn_id.as_str()).unwrap())
        .unwrap();
    let kinds = snapshot
        .facts
        .iter()
        .map(|fact| fact.kind.as_str())
        .collect::<Vec<_>>();
    let effect_start = kinds
        .iter()
        .position(|kind| *kind == "effect.prepared")
        .expect("effect prepared");
    assert_eq!(
        &kinds[effect_start..effect_start + 9],
        [
            "effect.prepared",
            "safety.decided",
            "effect.authorized",
            "sandbox.bound",
            "sandbox.preflighted",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
        ]
    );
    assert_eq!(
        &kinds[kinds.len() - 2..],
        ["execution.completed", "turn.completed"]
    );
}

#[tokio::test]
async fn restart_abandons_unproven_execution_before_dispatch() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let (dispatcher, queue) = local_dispatch_queue(1).expect("queue");
    drop(queue);
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "a".repeat(64),
            agent_instance_namespace: "local-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: Some(20),
                max_output_tokens: Some(10),
                deadline_budget_ms: None,
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        Arc::new(Clock),
        dispatcher,
    )
    .expect("host");
    let session = host
        .create_session("create-restart", "definition-main")
        .expect("session");
    let original = host
        .start_turn("start-restart", &session.session_id, "recover me")
        .expect("commit survives dispatch rejection");
    let mut ledger = SqliteLedger::open(&database).expect("restart ledger");
    let recovered =
        recover_local_dispatches(&mut ledger, 3, "2026-08-29T00:00:02Z").expect("recovery plan");
    assert_eq!(recovered.len(), 1);
    assert_ne!(recovered[0].execution_id.as_str(), original.execution_id);
    assert_eq!(recovered[0].session_version, 3);
    let kinds: Vec<_> = ledger
        .load_turn(&recovered[0].turn_id)
        .expect("turn snapshot")
        .facts
        .iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect();
    assert_eq!(kinds[3..], ["execution.abandoned", "execution.started"]);
    drop(ledger);

    let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new(&database, policy(), model.clone()).expect("worker");
    assert!(matches!(
        worker.execute(&recovered[0], &attempt()).await,
        Ok(LocalWorkerDisposition::TerminalCommitted { .. })
    ));
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    let page = host
        .read_event_page(&session.session_id, 0)
        .expect("terminal events");
    assert_eq!(
        page.events.last().expect("terminal").event,
        "turn.completed"
    );
}

#[test]
fn zero_capacity_is_rejected() {
    assert_eq!(
        local_dispatch_queue(0).err().expect("zero capacity"),
        LocalWorkerError::InvalidComposition
    );
}

#[tokio::test]
async fn shutdown_stops_admission_and_bounds_in_memory_drain() {
    let directory = tempdir().expect("tempdir");
    let database = directory.path().join("garive.db");
    let (dispatcher, mut queue) = local_dispatch_queue(1).expect("queue");
    let committed = CommittedTurn {
        session_id: SessionId::try_from("session").expect("session"),
        turn_id: TurnId::try_from("turn").expect("turn"),
        execution_id: ExecutionId::try_from("execution").expect("execution"),
        definition_id: "definition-main".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: "a".repeat(64),
        session_version: 1,
        committed_position: 1,
    };
    dispatcher.dispatch(&committed).expect("first admission");
    let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new(&database, policy(), model).expect("worker");
    let report = queue.shutdown_drain(&worker, &[]).await;
    assert_eq!(report.attempted, 0);
    assert_eq!(report.abandoned, 1);
    assert!(dispatcher.dispatch(&committed).is_err());
}
