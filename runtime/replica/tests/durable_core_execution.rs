use std::{
    num::NonZeroU32,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use futures::executor::block_on;
use garive_core::{
    AgentCursor, AgentDefinitionId, AgentDefinitionRevision, AgentEntry, AgentEvent,
    AgentInstanceId, AgentOutcome, AgentToolCapabilities, AgentTurnRequest, ClockPort, ContextItem,
    ContextPort, ContextPortError, ContextPurpose, ContextRequest, ContextSurface, EventSink,
    ExecutionId as CoreExecutionId, ExecutionLimits, FactRef, MissingUsagePolicy, ModelOnlyLimits,
    ModelRecoveryPolicy, OutputLimitAction, PortFailure, SessionId as CoreSessionId,
    TerminalRecoveryAction, TurnId as CoreTurnId,
};
use garive_ledger::{
    AgentDefinitionId as LedgerDefinitionId, AgentDefinitionRevision as LedgerRevision,
    AgentInstanceId as LedgerAgentId, CanonicalPayload, FactDraft, FactId, FactKind, SessionId,
};
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture,
    ModelInputContent, ModelInputItem, ModelItem, ModelObserver, ModelOutputSettings, ModelPort,
    ModelRequest, ModelStopReason, ModelTargetId, ModelUsage, TextMode, TokenCount, UsageSource,
};
use garive_runtime::{
    commit_planned_turn, execute_durable_agent, execute_durable_model_only, plan_cancel_turn,
    plan_start_turn, AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest,
    CancelReason, CancelTurnCommand, DurableExecutionConfig, DurableExecutionError,
    EffectiveRuntimeLimits, ExecutionLeaseRequest, ExecutorDispatch, ExecutorFuture, ExecutorPort,
    ModelLifecycleContext, PreparedExecution, RuntimeCommandError, RuntimeCommandId, SqliteLedger,
    StartTurnCommand, TerminalPublicationError, TerminalPublisher,
};
use garive_tools::{
    EffectReceipt, ExecutionCapability, ExecutionFact, ExecutionRequirements, ReceiptId,
    ReplayClass, TerminalClassification, ToolDefinition,
};
use tempfile::tempdir;

struct Context {
    positions: Vec<u64>,
}
impl ContextPort for Context {
    fn derive(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<ContextSurface, ContextPortError> {
        self.positions.push(request.through_position);
        let fact = FactRef {
            session_id: request.session_id.clone(),
            position: 3,
        };
        Ok(ContextSurface {
            purpose: ContextPurpose::Inference,
            from_position: 1,
            through_position: request.through_position,
            items: vec![ContextItem::Input {
                fact_ref: fact.clone(),
                item: ModelInputItem::Message {
                    role: garive_llm::ModelRole::User,
                    content: vec![ModelInputContent::Text("hello".into())],
                },
            }],
            retained_refs: vec![fact],
            dropped_refs: vec![],
            filtered_refs: vec![],
            item_count: 1,
            utf8_bytes: 5,
        })
    }
}

struct Model {
    path: PathBuf,
    session: SessionId,
    tool_first: bool,
    calls: AtomicUsize,
}
impl ModelPort for Model {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let ledger = SqliteLedger::open(&self.path).unwrap();
            let active = ledger.list_uncertain_model_requests(&self.session).unwrap();
            assert_eq!(active[0].as_str(), request.request_id.as_str());
            Ok(InvokeOutcome::Completed {
                items: if self.tool_first && call == 0 {
                    vec![ModelItem::ToolIntent {
                        model_call_id: "call".into(),
                        tool_name: "read_file".into(),
                        arguments_json: r#"{"path":"a"}"#.into(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "done".into(),
                    }]
                },
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: if self.tool_first && call == 0 {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

struct CancelDuringModel {
    path: PathBuf,
    session: SessionId,
    turn: garive_ledger::TurnId,
}

impl ModelPort for CancelDuringModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let mut ledger = SqliteLedger::open(&self.path).unwrap();
            let snapshot = ledger.load_turn(&self.turn).unwrap();
            let cancel = plan_cancel_turn(&CancelTurnCommand {
                command_id: RuntimeCommandId::new("cancel-during-model").unwrap(),
                session_id: self.session.clone(),
                turn_id: self.turn.clone(),
                reason: CancelReason::User,
                requested_through_position: snapshot.through_position,
                recorded_at: "2026-08-29T00:00:02Z".into(),
            })
            .unwrap();
            commit_planned_turn(
                &mut ledger,
                self.session.clone(),
                snapshot.session_version,
                &cancel,
            )
            .unwrap();
            assert!(cancellation.is_cancelled());
            Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::Cancelled,
                partial_items: vec![],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(0),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
            })
        })
    }
}

struct Authority;
impl AuthorityPort for Authority {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            Ok(AuthorityDecision::Approve {
                granted_requirements: request.prepared.requirements().clone(),
                constraints_digest: "a".repeat(64),
                authority_revision: "policy-1".into(),
            })
        })
    }
}

struct Executor {
    path: PathBuf,
    session: SessionId,
}
impl ExecutorPort for Executor {
    fn prepare(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        _: &garive_tools::PreparedToolCall,
        _: &garive_tools::InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        Ok(PreparedExecution {
            executor_id: "local.read".into(),
            executor_revision: "1".into(),
            dispatch_attempt_id: "tool-dispatch".into(),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        Box::pin(async move {
            let ledger = SqliteLedger::open(&self.path).unwrap();
            assert_eq!(
                ledger
                    .list_uncertain_tool_invocations(&self.session)
                    .unwrap()
                    .len(),
                1
            );
            let content = serde_json::json!({"text":"ok"});
            let result_digest = CanonicalPayload::from_value(&content)
                .unwrap()
                .sha256()
                .to_owned();
            Ok(ExecutionFact::Completed {
                receipt: Some(EffectReceipt {
                    receipt_id: ReceiptId::new(command.receipt_id).unwrap(),
                    invocation_id: command.invocation_id.clone(),
                    prepared_digest: command.prepared.input_digest().into(),
                    grant_id: command.grant.grant_id.clone(),
                    executor_id: command.execution.executor_id.clone(),
                    executor_revision: command.execution.executor_revision.clone(),
                    terminal_classification: TerminalClassification::Completed,
                    result_digest,
                }),
                content,
                truncated: false,
            })
        })
    }
}

struct Signals;
impl ModelCancellation for Signals {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl ClockPort for Signals {
    fn now_tick(&self) -> Result<u64, PortFailure> {
        Ok(0)
    }
}
impl EventSink for Signals {
    fn emit(&mut self, _: AgentEvent) -> Result<(), PortFailure> {
        Ok(())
    }
}

struct Publisher {
    path: PathBuf,
    turn: garive_ledger::TurnId,
    fail: bool,
    calls: usize,
    expected_terminal: [&'static str; 2],
}
impl TerminalPublisher for Publisher {
    fn publish_terminal(
        &mut self,
        _: &garive_core::ExecutionReport,
        positions: &[u64],
    ) -> Result<(), TerminalPublicationError> {
        let ledger = SqliteLedger::open(&self.path).unwrap();
        let snapshot = ledger.load_turn(&self.turn).unwrap();
        let kinds: Vec<_> = snapshot
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect();
        assert_eq!(&kinds[kinds.len() - 2..], self.expected_terminal);
        assert_eq!(positions.len(), 2);
        self.calls += 1;
        if self.fail {
            Err(TerminalPublicationError)
        } else {
            Ok(())
        }
    }
}

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("session-open").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&serde_json::json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

#[test]
fn sqlite_dispatch_and_publication_cross_only_after_their_commits() {
    for fail_publication in [false, true] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let session = SessionId::try_from(if fail_publication {
            "session-fail"
        } else {
            "session"
        })
        .unwrap();
        let mut ledger = SqliteLedger::open(&path).unwrap();
        ledger
            .commit(session.clone(), 0, vec![open_session()])
            .unwrap();
        let start = StartTurnCommand {
            command_id: RuntimeCommandId::new("start").unwrap(),
            session_id: session.clone(),
            agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
            definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
            definition_revision: LedgerRevision::try_from("revision").unwrap(),
            snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .into(),
            trusted_input: "hello".into(),
            limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            recorded_at: "2026-08-29T00:00:00Z".into(),
        };
        let plan = plan_start_turn(&start, 1).unwrap();
        let execution = plan.execution_id.clone().unwrap();
        ledger.commit(session.clone(), 1, plan.facts).unwrap();
        let request = core_request(&session, &plan.turn_id, &execution);
        let config = DurableExecutionConfig {
            session_id: session.clone(),
            expected_session_version: 2,
            model: ModelLifecycleContext {
                turn_id: plan.turn_id.clone(),
                execution_id: execution.clone(),
                deployment_id: "deployment".into(),
                recovery_policy_revision: "policy".into(),
                max_attempts: 1,
                recorded_at: "2026-08-29T00:00:01Z".into(),
            },
            lease: ExecutionLeaseRequest {
                turn_id: plan.turn_id.clone(),
                execution_id: execution,
                owner_id: "test-worker".into(),
                lease_token: format!("lease-{fail_publication}"),
                now_ms: 1,
                duration_ms: 10_000,
            },
        };
        let model = Model {
            path: path.clone(),
            session,
            tool_first: false,
            calls: AtomicUsize::new(0),
        };
        let mut context = Context { positions: vec![] };
        let signals = Signals;
        let mut events = Signals;
        let mut publisher = Publisher {
            path: path.clone(),
            turn: plan.turn_id.clone(),
            fail: fail_publication,
            calls: 0,
            expected_terminal: ["execution.completed", "turn.completed"],
        };
        let mut stale = request.clone();
        stale.context_request.through_position -= 1;
        assert!(matches!(
            block_on(execute_durable_model_only(
                &mut ledger,
                &config,
                &stale,
                &mut context,
                &model,
                &mut events,
                &signals,
                &signals,
                &mut publisher,
            )),
            Err(DurableExecutionError::Command(
                RuntimeCommandError::ConcurrentModification
            ))
        ));
        let result = block_on(execute_durable_model_only(
            &mut ledger,
            &config,
            &request,
            &mut context,
            &model,
            &mut events,
            &signals,
            &signals,
            &mut publisher,
        ))
        .unwrap();
        assert_eq!(result.publication.is_err(), fail_publication);
        assert_eq!(publisher.calls, 1);
        assert_eq!(ledger.load_turn(&plan.turn_id).unwrap().facts.len(), 9);
    }
}

#[test]
fn complete_agent_loop_coordinates_model_effect_context_and_terminal_commits() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("agent.sqlite3");
    let session = SessionId::try_from("agent-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("agent-start").unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 3,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, plan.facts).unwrap();
    let mut request = core_request(&session, &plan.turn_id, &execution);
    request.required_capabilities = vec![ModelCapability::Tools];
    request.limits.execution = ExecutionLimits::new(NonZeroU32::new(3).unwrap());
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "deployment".into(),
            recovery_policy_revision: "policy".into(),
            max_attempts: 1,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution,
            owner_id: "agent-worker".into(),
            lease_token: "agent-lease".into(),
            now_ms: 1,
            duration_ms: 10_000,
        },
    };
    let model = Model {
        path: path.clone(),
        session: session.clone(),
        tool_first: true,
        calls: AtomicUsize::new(0),
    };
    let mut authority = Authority;
    let mut executor = Executor {
        path: path.clone(),
        session: session.clone(),
    };
    let requirements =
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 1_024).unwrap();
    let capabilities = AgentToolCapabilities {
        definitions: vec![ToolDefinition::new(
            "read_file",
            "1",
            "Read one file.",
            serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
            requirements,
            ReplayClass::ReadOnly,
        )
        .unwrap()],
    };
    let mut context = Context { positions: vec![] };
    let signals = Signals;
    let mut events = Signals;
    let mut publisher = Publisher {
        path: path.clone(),
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.completed", "turn.completed"],
    };
    let result = block_on(execute_durable_agent(
        &mut ledger,
        &config,
        &request,
        &capabilities,
        &mut context,
        &model,
        &mut authority,
        &mut executor,
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    ))
    .unwrap();
    assert!(matches!(
        result.report.outcome,
        AgentOutcome::Completed { .. }
    ));
    assert_eq!(context.positions, [4, 14]);
    assert_eq!(publisher.calls, 1);
    let kinds = ledger
        .load_turn(&plan.turn_id)
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "turn.started",
            "turn.input",
            "execution.started",
            "execution.iteration_started",
            "model.prepared",
            "model.started",
            "model.completed",
            "effect.prepared",
            "effect.authorized",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
            "execution.iteration_started",
            "model.prepared",
            "model.started",
            "model.completed",
            "execution.completed",
            "turn.completed",
        ]
    );
}

#[test]
fn durable_cancel_request_reaches_the_frozen_core_signal() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("cancel.sqlite3");
    let session = SessionId::try_from("cancel-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("cancel-start").unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger
        .commit(session.clone(), 1, plan.facts.clone())
        .unwrap();
    let request = core_request(&session, &plan.turn_id, &execution);
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "deployment".into(),
            recovery_policy_revision: "policy".into(),
            max_attempts: 1,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution,
            owner_id: "cancel-worker".into(),
            lease_token: "cancel-lease".into(),
            now_ms: 1,
            duration_ms: 10_000,
        },
    };
    let model = CancelDuringModel {
        path: path.clone(),
        session,
        turn: plan.turn_id.clone(),
    };
    let mut context = Context { positions: vec![] };
    let signals = Signals;
    let mut events = Signals;
    let mut publisher = Publisher {
        path,
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.stopped", "turn.stopped"],
    };
    let result = block_on(execute_durable_model_only(
        &mut ledger,
        &config,
        &request,
        &mut context,
        &model,
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    ))
    .unwrap();
    assert!(matches!(
        result.report.outcome,
        AgentOutcome::Stopped {
            reason: garive_core::StopReason::Cancelled
        }
    ));
    let kinds = ledger
        .load_turn(&plan.turn_id)
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    assert!(kinds
        .windows(2)
        .any(|pair| pair == ["model.started", "turn.cancel_requested"]));
    assert_eq!(
        &kinds[kinds.len() - 2..],
        ["execution.stopped", "turn.stopped"]
    );
}

fn core_request(
    session: &SessionId,
    turn: &garive_ledger::TurnId,
    execution: &garive_ledger::ExecutionId,
) -> AgentTurnRequest {
    AgentTurnRequest {
        session_id: CoreSessionId::try_from(session.as_str()).unwrap(),
        turn_id: CoreTurnId::try_from(turn.as_str()).unwrap(),
        execution_id: CoreExecutionId::try_from(execution.as_str()).unwrap(),
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        entry: AgentEntry::Start {
            trusted_input: "hello".into(),
        },
        cursor: AgentCursor {
            completed_iterations: 0,
            last_durable_position: 0,
        },
        context_request: ContextRequest {
            session_id: session.as_str().into(),
            turn_id: turn.as_str().into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: 4,
            max_items: 10,
            max_utf8_bytes: 100,
        },
        model_targets: vec![ModelTargetId::new("target")],
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
        limits: ModelOnlyLimits {
            execution: ExecutionLimits::new(NonZeroU32::new(2).unwrap()),
            max_total_tokens: Some(10),
            deadline_tick: None,
        },
    }
}
