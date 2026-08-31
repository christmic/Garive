use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use futures::executor::block_on;
use garive_core::{
    AgentOutcome, AgentToolCapabilities, ExecutionReport, GovernedEffectPort,
    GovernedSuspensionBinding, SuspensionReason, ToolPreparationPort, UsageSummary,
};
use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, SessionId, TurnId,
};
use garive_llm::{
    InvokeOutcome, ModelCapability, ModelItem, ModelOutputSettings, ModelRequest, ModelRequestId,
    ModelStopReason, ModelTargetId, ModelUsage, TextMode, TokenCount, ToolDescriptor, UsageSource,
};
use garive_runtime::{
    derive_runtime_recovery, plan_core_terminal, plan_f0_safety_decision,
    plan_f0_sandbox_admission, plan_model_prepared, plan_model_started, plan_model_terminal,
    reconstruct_suspended_turn, recover_f0_prepared, recover_local_dispatches_with_f0,
    ActivityProjectionLimits, AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest,
    ContinuationInput, ContinueTurnCommand, CoreTerminalContext, EffectRecoveryPosition,
    ExecutorDispatch, ExecutorFuture, ExecutorPort, ExecutorRecoveryRequest,
    F0EffectAdmissionContext, F0GovernanceContext, F0RecoveryContentPort, F0RecoveryError,
    F0SafetyDecisionContext, GovernedEffectConfig, HostClock, HostReadLimits,
    InstalledActivityCatalogue, InstalledActivityDescriptor, InstalledAgent,
    InteractionInputRepresentation, LiveHost, LiveHostLimits, LocalF0Governance,
    LocalGovernedExecution, LocalGovernedExecutionFactory, LocalRecoveryError, LocalWorkerError,
    ModelLifecycleContext, PreparedExecution, RuntimeCommandError, RuntimeCommandId,
    SafetyDecisionV1, SafetyDisposition, SafetyEvaluation, SafetyFuture, SafetyInteraction,
    SafetyPort, SandboxAdmission, SandboxAdmissionPort, SandboxAdmissionRequest, SandboxBindingV1,
    SqliteGovernedEffectPort, SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, BuiltinT1Catalogue, EffectReceipt,
    ExecutionCapability, ExecutionFact, ExecutionRequirements, GrantId, InteractionKind,
    InvocationAccessSet, InvocationGrant, PreparationError, ReceiptId, ReplayClass, ResourceAccess,
    SandboxControl, SandboxRequirementsV1, TerminalClassification, ToolAccessPolicyV1,
    ToolAccessResolver, ToolCatalog, ToolDefinition, ToolIntent, T1_PROCESS_RUN,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[derive(Clone, Copy)]
enum Decision {
    Approve,
    Interaction,
    Deny,
}

struct TestClock;

impl HostClock for TestClock {
    fn recorded_at(&self) -> String {
        timestamp().to_owned()
    }
}

struct NoopDispatcher;

impl TurnDispatcher for NoopDispatcher {
    fn dispatch(&self, _: &garive_runtime::CommittedTurn) -> Result<(), TurnDispatchError> {
        Ok(())
    }
}

struct Authority {
    decision: Decision,
}

struct AllowSafety(&'static str);

struct UnavailableSafety;

impl SafetyPort for UnavailableSafety {
    fn decide<'a>(&'a mut self, _: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async { Err(garive_runtime::GovernedRuntimePortError::AuthorityUnavailable) })
    }
}

impl SafetyPort for AllowSafety {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        let constraint = self.0;
        Box::pin(async move {
            Ok(SafetyEvaluation {
                decision: SafetyDecisionV1::new(
                    "safety-decision",
                    SafetyDisposition::Allow,
                    request.invocation_id().clone(),
                    request.prepared_digest(),
                    Some(constraint.repeat(64)),
                    request.effective_policy_revision(),
                    None,
                )
                .unwrap(),
                granted_requirements: Some(
                    ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 1_024)
                        .unwrap(),
                ),
                interaction: None,
            })
        })
    }
}

struct LocalSandbox(&'static str);

impl SandboxAdmissionPort for LocalSandbox {
    fn admit(
        &mut self,
        _: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, garive_runtime::GovernedRuntimePortError> {
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                "binding",
                "workspace",
                "local.read",
                self.0,
                "policy-1",
                access_policy(),
                sandbox_requirements(),
            )
            .unwrap(),
            effective_limits_digest: "e".repeat(64),
            preflight_id: "preflight".into(),
            dispatch_attempt_id: "dispatch-1".into(),
        })
    }
}

struct Resolver;

impl ToolAccessResolver for Resolver {
    fn revision(&self) -> &str {
        "resolver-1"
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap(),
            AccessMode::Read,
        )?])
    }
}

struct NoReferencedContent;

impl F0RecoveryContentPort for NoReferencedContent {
    fn resolve(&mut self, _: &str) -> Result<String, F0RecoveryError> {
        Err(F0RecoveryError::ContentUnavailable)
    }
}

struct RecoveryPreparation;
impl ToolPreparationPort for RecoveryPreparation {
    fn prepare(
        &self,
        intent: &ToolIntent,
    ) -> Result<garive_tools::PreparedToolCall, PreparationError> {
        v3_catalog().prepare_v3(intent, &Resolver)
    }
}

struct RecoverySafety(Decision);
impl SafetyPort for RecoverySafety {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        let decision = self.0;
        Box::pin(async move {
            match decision {
                Decision::Interaction => Ok(SafetyEvaluation {
                    decision: SafetyDecisionV1::new(
                        "safety-interaction",
                        SafetyDisposition::InteractionRequired,
                        request.invocation_id().clone(),
                        request.prepared_digest(),
                        None,
                        request.effective_policy_revision(),
                        Some("safety_interaction_required".into()),
                    )
                    .unwrap(),
                    granted_requirements: None,
                    interaction: Some(SafetyInteraction {
                        kind: InteractionKind::Approval,
                        prompt: json!({"schema_version":1,"title_key":"approval.title","action_label_key":"approval.allow"}),
                        response_schema: json!({"type":"boolean"}),
                        expiry_code: "none".into(),
                    }),
                }),
                _ => AllowSafety("a").decide(request).await,
            }
        })
    }
}

struct RecoveryFactory {
    decision: Decision,
    mode: ExecutionMode,
}
impl LocalGovernedExecutionFactory for RecoveryFactory {
    fn create(
        &self,
        _: &garive_runtime::CommittedTurn,
    ) -> Result<LocalGovernedExecution, LocalWorkerError> {
        Ok(LocalGovernedExecution {
            capabilities: AgentToolCapabilities {
                definitions: vec![v3_definition()],
            },
            authority: Box::new(Authority {
                decision: self.decision,
            }),
            executor: Box::new(Executor {
                mode: self.mode,
                prepares: 0,
                dispatches: 0,
            }),
            f0: LocalF0Governance {
                preparation: Box::new(RecoveryPreparation),
                recovery_content: Box::new(NoReferencedContent),
                safety: Box::new(RecoverySafety(self.decision)),
                sandbox: Box::new(LocalSandbox("1")),
                context: F0GovernanceContext {
                    actor_authority_reference: "actor".into(),
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: "policy-1".into(),
                },
            },
        })
    }
}

struct LossRecoveryFactory {
    reconciliations: Arc<AtomicUsize>,
    fail: bool,
}

impl LocalGovernedExecutionFactory for LossRecoveryFactory {
    fn create(
        &self,
        _: &garive_runtime::CommittedTurn,
    ) -> Result<LocalGovernedExecution, LocalWorkerError> {
        Ok(LocalGovernedExecution {
            capabilities: AgentToolCapabilities {
                definitions: vec![v3_definition()],
            },
            authority: Box::new(Authority {
                decision: Decision::Approve,
            }),
            executor: Box::new(LossExecutor {
                reconciliations: Arc::clone(&self.reconciliations),
                fail: self.fail,
            }),
            f0: LocalF0Governance {
                preparation: Box::new(RecoveryPreparation),
                recovery_content: Box::new(NoReferencedContent),
                safety: Box::new(RecoverySafety(Decision::Approve)),
                sandbox: Box::new(LocalSandbox("1")),
                context: F0GovernanceContext {
                    actor_authority_reference: "actor".into(),
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: "policy-1".into(),
                },
            },
        })
    }
}

struct LossExecutor {
    reconciliations: Arc<AtomicUsize>,
    fail: bool,
}

impl ExecutorPort for LossExecutor {
    fn prepare(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        _: &garive_tools::PreparedToolCall,
        _: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        Err("recovery-only executor".into())
    }

    fn dispatch<'a>(&'a mut self, _: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        Box::pin(async { Err(garive_runtime::ExecutorDispatchError::ExecutorStateUnknown) })
    }

    fn reconcile_started_loss(
        &mut self,
        request: ExecutorRecoveryRequest<'_>,
    ) -> Result<(), garive_runtime::ExecutorDispatchError> {
        assert_eq!(request.invocation_id.as_str(), "f0-recovery");
        assert_eq!(request.executor_id, "garive.builtin.process");
        assert_eq!(request.executor_revision, "process-v1");
        assert_eq!(request.dispatch_attempt_id, "process-dispatch-1");
        self.reconciliations.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(garive_runtime::ExecutorDispatchError::ExecutorStateUnknown)
        } else {
            Ok(())
        }
    }
}

impl AuthorityPort for Authority {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            Ok(match self.decision {
                Decision::Approve => AuthorityDecision::Approve {
                    granted_requirements: request.prepared.requirements().clone(),
                    constraints_digest: "a".repeat(64),
                    authority_revision: "policy-1".into(),
                },
                Decision::Interaction => AuthorityDecision::InteractionRequired {
                    kind: InteractionKind::Approval,
                    prompt: json!({"schema_version":1,"title_key":"approval.title","message_text":"approve","action_label_key":"approval.allow","cancel_label_key":"approval.deny"}),
                    response_schema: json!({"type":"boolean"}),
                    expiry_code: "none".into(),
                },
                Decision::Deny => AuthorityDecision::Deny {
                    safe_details: Some("policy denied".into()),
                },
            })
        })
    }
}

#[derive(Clone, Copy)]
enum ExecutionMode {
    Success,
    AcknowledgeFailure,
    InvalidReceipt,
    Unsupported,
}

struct Executor {
    mode: ExecutionMode,
    prepares: usize,
    dispatches: usize,
}

impl ExecutorPort for Executor {
    fn prepare(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        _: &garive_tools::PreparedToolCall,
        _: &garive_tools::InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        self.prepares += 1;
        if matches!(self.mode, ExecutionMode::Unsupported) {
            return Err("network".into());
        }
        Ok(PreparedExecution {
            executor_id: "local.read".into(),
            executor_revision: "1".into(),
            dispatch_attempt_id: "dispatch-1".into(),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        self.dispatches += 1;
        Box::pin(async move {
            let content = json!({"text":"ok"});
            let result_digest = CanonicalPayload::from_value(&content)
                .unwrap()
                .sha256()
                .to_owned();
            let receipt_id = match self.mode {
                ExecutionMode::Success
                | ExecutionMode::AcknowledgeFailure
                | ExecutionMode::Unsupported => command.receipt_id,
                ExecutionMode::InvalidReceipt => "wrong-receipt",
            };
            Ok(ExecutionFact::Completed {
                receipt: Some(EffectReceipt {
                    receipt_id: ReceiptId::new(receipt_id).unwrap(),
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

    fn acknowledge_receipt(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        _: &EffectReceipt,
    ) -> Result<(), garive_runtime::ExecutorDispatchError> {
        if matches!(self.mode, ExecutionMode::AcknowledgeFailure) {
            Err(garive_runtime::ExecutorDispatchError::ExecutorStateUnknown)
        } else {
            Ok(())
        }
    }
}

struct Setup {
    database: PathBuf,
    ledger: SqliteLedger,
    session: SessionId,
    turn: TurnId,
    execution: ExecutionId,
    request_id: String,
    prepared: garive_tools::PreparedToolCall,
    version: u64,
    position: u64,
}

fn setup(path: &std::path::Path) -> Setup {
    let session = SessionId::try_from("session").unwrap();
    let turn = TurnId::try_from("t1").unwrap();
    let execution = ExecutionId::try_from("e1").unwrap();
    let mut ledger = SqliteLedger::open(path).unwrap();
    let initial = vec![
        draft(
            "f1",
            "session.opened",
            None,
            None,
            json!({
                "command_id":"open","definition_id":"definition",
                "definition_revision":"revision","snapshot_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "agent_instance_id":"agent"
            }),
        ),
        draft(
            "f2",
            "turn.started",
            Some(&turn),
            None,
            runtime_payload("turn.started"),
        ),
        draft(
            "f-input",
            "turn.input",
            Some(&turn),
            None,
            runtime_payload("turn.input"),
        ),
        draft(
            "f3",
            "execution.started",
            Some(&turn),
            Some(&execution),
            runtime_payload("execution.started"),
        ),
    ];
    let committed = ledger.commit(session.clone(), 0, initial).unwrap();
    let request = model_request();
    let lifecycle = ModelLifecycleContext {
        turn_id: turn.clone(),
        execution_id: execution.clone(),
        deployment_id: "deployment".into(),
        recovery_policy_revision: "recovery-1".into(),
        max_attempts: 1,
        recorded_at: timestamp().into(),
    };
    let prepared_request = plan_model_prepared(&lifecycle, &request).unwrap();
    let committed = ledger
        .commit(
            session.clone(),
            committed.session_version,
            vec![prepared_request.fact.clone()],
        )
        .unwrap();
    let started = plan_model_started(&lifecycle, &prepared_request, "model-dispatch").unwrap();
    let committed = ledger
        .commit(session.clone(), committed.session_version, vec![started])
        .unwrap();
    let outcome = InvokeOutcome::Completed {
        items: vec![ModelItem::ToolIntent {
            model_call_id: "call".into(),
            tool_name: "read_file".into(),
            arguments_json: r#"{"path":"a"}"#.into(),
        }],
        usage: usage(),
        stop_reason: ModelStopReason::ToolUse,
    };
    let terminal = plan_model_terminal(&lifecycle, &prepared_request, &outcome).unwrap();
    let committed = ledger
        .commit(session.clone(), committed.session_version, vec![terminal])
        .unwrap();
    Setup {
        database: path.to_owned(),
        ledger,
        session,
        turn,
        execution,
        request_id: request.request_id.as_str().into(),
        prepared: tool_catalog()
            .prepare(&ToolIntent::new("call", "read_file", r#"{"path":"a"}"#))
            .unwrap(),
        version: committed.session_version,
        position: *committed.positions.last().unwrap(),
    }
}

#[test]
fn sqlite_success_commits_every_effect_boundary_before_observation() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let initial_position = setup.position;
    let mut port = port(&mut setup, &mut authority, &mut executor);
    let result = block_on(port.invoke(&request_id, &prepared)).unwrap();
    assert!(matches!(
        result.result,
        garive_tools::GovernedToolResult::Observation(_)
    ));
    assert_eq!(result.through_position, initial_position + 6);
    assert!(result.suspension_binding.is_none());
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (1, 1));
    assert_tail(
        &setup,
        &[
            "effect.prepared",
            "effect.authorized",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
        ],
    );
    let host = LiveHost::new_with_read_limits(
        &setup.database,
        InstalledAgent {
            definition_id: "definition".into(),
            definition_revision: "revision".into(),
            snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .into(),
            agent_instance_namespace: "agent".into(),
            public_capabilities: Vec::new(),
            runtime_limits: garive_runtime::EffectiveRuntimeLimits {
                max_iterations: 4,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            public_activity_catalogue: Some(InstalledActivityCatalogue {
                schema_version: 1,
                catalogue_revision: "catalogue-1".into(),
                descriptors: vec![InstalledActivityDescriptor {
                    tool_name: "read_file".into(),
                    tool_revision: "1".into(),
                    label_key: "agent.activity.read_file".into(),
                }],
            }),
        },
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: Some(ActivityProjectionLimits {
                max_activities_per_turn: 8,
                max_activity_facts: 64,
                max_label_bytes: 128,
                max_activity_id_bytes: 128,
                max_encoded_bytes_per_turn: 8_192,
            }),
        },
        HostReadLimits::PRODUCT_DEFAULT,
        Arc::new(TestClock),
        Arc::new(NoopDispatcher),
    )
    .unwrap();
    let timeline = host.get_timeline(setup.session.as_str(), 0, 10).unwrap();
    let activity = &timeline.items[0].activities[0];
    assert_eq!(activity.kind, "tool");
    assert_eq!(activity.label_key, "agent.activity.read_file");
    assert_eq!(activity.state, "completed");
    assert!(activity.terminal);
    assert!(activity.safe_code.is_none());
    let events = host
        .read_event_page(setup.session.as_str(), initial_position)
        .unwrap();
    assert_eq!(
        events
            .events
            .iter()
            .map(|event| event.event.as_str())
            .collect::<Vec<_>>(),
        [
            "agent.activity.prepared",
            "agent.activity.authorized",
            "agent.activity.started",
            "agent.activity.completed",
        ]
    );
}

#[test]
fn receipt_acknowledgement_failure_stops_after_the_durable_receipt() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("receipt-ack.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::AcknowledgeFailure,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);

    assert!(block_on(port.invoke(&request_id, &prepared)).is_err());
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (1, 1));
    assert_tail(
        &setup,
        &[
            "effect.prepared",
            "effect.authorized",
            "effect.started",
            "effect.receipt",
        ],
    );
    assert_eq!(
        recover_local_dispatches_with_f0(
            &mut setup.ledger,
            3,
            timestamp(),
            &RecoveryFactory {
                decision: Decision::Approve,
                mode: ExecutionMode::AcknowledgeFailure,
            },
            1_024,
        ),
        Err(LocalRecoveryError::F0RecoveryFailed)
    );
    assert!(!setup
        .ledger
        .load_turn(&setup.turn)
        .unwrap()
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "effect.completed"));
    recover_local_dispatches_with_f0(
        &mut setup.ledger,
        3,
        timestamp(),
        &RecoveryFactory {
            decision: Decision::Approve,
            mode: ExecutionMode::Success,
        },
        1_024,
    )
    .unwrap();
    assert!(setup
        .ledger
        .load_turn(&setup.turn)
        .unwrap()
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "effect.completed"));
}

#[test]
fn prepared_v3_without_f0_brokers_fails_before_any_effect_fact() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    setup.prepared = v3_catalog()
        .prepare_v3(
            &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
            &Resolver,
        )
        .unwrap();
    let before = setup.ledger.load_turn(&setup.turn).unwrap().facts.len();
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);
    assert!(block_on(port.invoke(&request_id, &prepared)).is_err());
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (0, 0));
    assert_eq!(
        setup.ledger.load_turn(&setup.turn).unwrap().facts.len(),
        before
    );
}

#[test]
fn prepared_v3_commits_exact_f0_chain_before_dispatch() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    setup.prepared = v3_catalog()
        .prepare_v3(
            &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
            &Resolver,
        )
        .unwrap();
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let mut safety = AllowSafety("a");
    let mut sandbox = LocalSandbox("1");
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor)
        .with_f0_governance(
            &mut safety,
            &mut sandbox,
            F0GovernanceContext {
                actor_authority_reference: "actor".into(),
                goal_reference: Some("goal:1".into()),
                plan_reference: Some("plan:1".into()),
                effective_policy_revision: "policy-1".into(),
            },
        )
        .unwrap();
    let result = block_on(port.invoke(&request_id, &prepared)).unwrap();
    assert!(matches!(
        result.result,
        garive_tools::GovernedToolResult::Observation(_)
    ));
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (0, 1));
    assert_tail(
        &setup,
        &[
            "effect.prepared",
            "safety.decided",
            "effect.authorized",
            "sandbox.bound",
            "sandbox.preflighted",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
        ],
    );
    let snapshot = setup.ledger.load_turn(&setup.turn).unwrap();
    let invocation = snapshot
        .facts
        .iter()
        .find(|fact| fact.kind.as_str() == "effect.prepared")
        .unwrap()
        .tool_invocation_id
        .as_ref()
        .unwrap()
        .as_str()
        .to_owned();
    let mut content = NoReferencedContent;
    let recovered = recover_f0_prepared(
        &snapshot,
        &invocation,
        &v3_catalog(),
        &Resolver,
        &mut content,
        1_024,
    )
    .unwrap();
    assert_eq!(recovered.prepared, prepared);
    assert_eq!(
        recover_f0_prepared(
            &snapshot,
            &invocation,
            &v3_catalog(),
            &Resolver,
            &mut content,
            1,
        ),
        Err(F0RecoveryError::ContentLimitExceeded)
    );
    drop(setup.ledger);
    let reopened = SqliteLedger::open(&setup.database).unwrap();
    let facts = reopened.load_turn(&setup.turn).unwrap().facts;
    assert_eq!(facts[facts.len() - 9].schema_version, 3);
    assert_eq!(facts[facts.len() - 7].schema_version, 2);
}

#[test]
fn prepared_v3_is_durable_before_safety_dependency_returns() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    setup.prepared = v3_catalog()
        .prepare_v3(
            &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
            &Resolver,
        )
        .unwrap();
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let mut safety = UnavailableSafety;
    let mut sandbox = LocalSandbox("1");
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor)
        .with_f0_governance(
            &mut safety,
            &mut sandbox,
            F0GovernanceContext {
                actor_authority_reference: "actor".into(),
                goal_reference: None,
                plan_reference: None,
                effective_policy_revision: "policy-1".into(),
            },
        )
        .unwrap();
    assert!(block_on(port.invoke(&request_id, &prepared)).is_err());
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (0, 0));
    assert_tail(&setup, &["effect.prepared"]);
    let recovery =
        derive_runtime_recovery(&setup.ledger.load_turn(&setup.turn).unwrap(), 3).unwrap();
    assert_eq!(recovery.effect, EffectRecoveryPosition::F0SafetyPending);
}

#[test]
fn restart_classifies_every_prestarted_f0_cut() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let prepared = v3_catalog()
        .prepare_v3(
            &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
            &Resolver,
        )
        .unwrap();
    let facts = f0_recovery_facts(&setup, &prepared);
    let expected = [
        EffectRecoveryPosition::F0SafetyPending,
        EffectRecoveryPosition::F0Decision,
        EffectRecoveryPosition::F0Authorized,
        EffectRecoveryPosition::F0SandboxBound,
        EffectRecoveryPosition::F0Preflighted,
    ];
    let mut version = setup.version;
    for (fact, expected) in facts.into_iter().zip(expected) {
        version = setup
            .ledger
            .commit(setup.session.clone(), version, vec![fact])
            .unwrap()
            .session_version;
        let recovered =
            derive_runtime_recovery(&setup.ledger.load_turn(&setup.turn).unwrap(), 3).unwrap();
        assert_eq!(recovered.effect, expected);
    }
}

#[test]
fn configured_brokers_resume_every_prestarted_f0_cut_without_duplicate_facts() {
    for cut in 1..=5 {
        let directory = tempdir().unwrap();
        let mut setup = setup(&directory.path().join(format!("resume-{cut}.sqlite3")));
        let prepared = v3_catalog()
            .prepare_v3(
                &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
                &Resolver,
            )
            .unwrap();
        let facts = f0_recovery_facts(&setup, &prepared);
        let committed = setup
            .ledger
            .commit(setup.session.clone(), setup.version, facts[..cut].to_vec())
            .unwrap();
        setup.version = committed.session_version;
        setup.position = *committed.positions.last().unwrap();
        let snapshot = setup.ledger.load_turn(&setup.turn).unwrap();
        let mut content = NoReferencedContent;
        let recovered = recover_f0_prepared(
            &snapshot,
            "f0-recovery",
            &v3_catalog(),
            &Resolver,
            &mut content,
            1_024,
        )
        .unwrap();
        let mut authority = Authority {
            decision: Decision::Approve,
        };
        let mut executor = Executor {
            mode: ExecutionMode::Success,
            prepares: 0,
            dispatches: 0,
        };
        let mut safety = AllowSafety("a");
        let mut sandbox = LocalSandbox("1");
        let mut port = port(&mut setup, &mut authority, &mut executor)
            .with_f0_governance(
                &mut safety,
                &mut sandbox,
                F0GovernanceContext {
                    actor_authority_reference: "actor".into(),
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: "policy-1".into(),
                },
            )
            .unwrap();
        let result = block_on(port.resume_f0(&snapshot, recovered)).unwrap();
        assert!(matches!(
            result.result,
            garive_tools::GovernedToolResult::Observation(_)
        ));
        drop(port);
        assert_eq!((executor.prepares, executor.dispatches), (0, 1));
        let facts = setup.ledger.load_turn(&setup.turn).unwrap().facts;
        for kind in [
            "effect.prepared",
            "safety.decided",
            "effect.authorized",
            "sandbox.bound",
            "sandbox.preflighted",
            "effect.started",
        ] {
            assert_eq!(
                facts
                    .iter()
                    .filter(|fact| fact.kind.as_str() == kind)
                    .count(),
                1,
                "cut {cut} duplicated {kind}"
            );
        }
    }
}

#[test]
fn changed_safety_or_sandbox_binding_never_dispatches_during_f0_resume() {
    for (cut, constraint, executor_revision) in [(2, "b", "1"), (5, "a", "2")] {
        let directory = tempdir().unwrap();
        let mut setup = setup(&directory.path().join(format!("changed-{cut}.sqlite3")));
        let prepared = v3_catalog()
            .prepare_v3(
                &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
                &Resolver,
            )
            .unwrap();
        let committed = setup
            .ledger
            .commit(
                setup.session.clone(),
                setup.version,
                f0_recovery_facts(&setup, &prepared)[..cut].to_vec(),
            )
            .unwrap();
        setup.version = committed.session_version;
        setup.position = *committed.positions.last().unwrap();
        let snapshot = setup.ledger.load_turn(&setup.turn).unwrap();
        let before = snapshot.facts.len();
        let mut content = NoReferencedContent;
        let recovered = recover_f0_prepared(
            &snapshot,
            "f0-recovery",
            &v3_catalog(),
            &Resolver,
            &mut content,
            1_024,
        )
        .unwrap();
        let mut authority = Authority {
            decision: Decision::Approve,
        };
        let mut executor = Executor {
            mode: ExecutionMode::Success,
            prepares: 0,
            dispatches: 0,
        };
        let mut safety = AllowSafety(constraint);
        let mut sandbox = LocalSandbox(executor_revision);
        let mut port = port(&mut setup, &mut authority, &mut executor)
            .with_f0_governance(
                &mut safety,
                &mut sandbox,
                F0GovernanceContext {
                    actor_authority_reference: "actor".into(),
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: "policy-1".into(),
                },
            )
            .unwrap();
        assert!(block_on(port.resume_f0(&snapshot, recovered)).is_err());
        drop(port);
        assert_eq!((executor.prepares, executor.dispatches), (0, 0));
        assert_eq!(
            setup.ledger.load_turn(&setup.turn).unwrap().facts.len(),
            before
        );
    }
}

#[test]
fn local_startup_resumes_all_f0_cuts_then_restarts_with_consumed_iteration() {
    for cut in 1..=5 {
        let directory = tempdir().unwrap();
        let mut setup = setup(&directory.path().join(format!("startup-{cut}.sqlite3")));
        let prepared = v3_catalog()
            .prepare_v3(
                &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
                &Resolver,
            )
            .unwrap();
        setup
            .ledger
            .commit(
                setup.session.clone(),
                setup.version,
                f0_recovery_facts(&setup, &prepared)[..cut].to_vec(),
            )
            .unwrap();
        let dispatches = recover_local_dispatches_with_f0(
            &mut setup.ledger,
            3,
            timestamp(),
            &RecoveryFactory {
                decision: Decision::Approve,
                mode: ExecutionMode::Success,
            },
            1_024,
        )
        .unwrap();
        assert_eq!(dispatches.len(), 1);
        let snapshot = setup.ledger.load_turn(&setup.turn).unwrap();
        assert_eq!(
            snapshot
                .facts
                .iter()
                .filter(|fact| fact.kind.as_str() == "effect.started")
                .count(),
            1
        );
        let replacement = snapshot
            .facts
            .iter()
            .rfind(|fact| fact.kind.as_str() == "execution.started")
            .unwrap();
        assert_eq!(payload(replacement)["completed_iterations"], 1);
        assert_eq!(
            replacement.execution_id.as_ref(),
            Some(&dispatches[0].execution_id)
        );
    }
}

#[test]
fn fresh_process_resumes_every_killed_f0_boundary_from_ledger_only() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (checkpoint, expected) in [
        ("f0_prepared", EffectRecoveryPosition::F0SafetyPending),
        ("f0_safety_decided", EffectRecoveryPosition::F0Decision),
        ("f0_authorized", EffectRecoveryPosition::F0Authorized),
        ("f0_sandbox_bound", EffectRecoveryPosition::F0SandboxBound),
        ("f0_preflighted", EffectRecoveryPosition::F0Preflighted),
    ] {
        let directory = tempdir().unwrap();
        let database = directory.path().join(format!("{checkpoint}.sqlite3"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_garive-runtime-crash-fixture"))
            .args([
                database.to_str().unwrap(),
                repository.to_str().unwrap(),
                checkpoint,
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "READY", "{checkpoint}");
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success(), "{checkpoint}");

        let mut ledger = SqliteLedger::open(&database).unwrap();
        let turn_id: String = ledger
            .connection_for_test()
            .query_row(
                "SELECT turn_id FROM ledger_facts WHERE kind='turn.started'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let before = ledger
            .load_turn(&TurnId::try_from(turn_id.as_str()).unwrap())
            .unwrap();
        assert_eq!(
            derive_runtime_recovery(&before, 3).unwrap().effect,
            expected
        );
        let dispatches = recover_local_dispatches_with_f0(
            &mut ledger,
            3,
            timestamp(),
            &RecoveryFactory {
                decision: Decision::Approve,
                mode: ExecutionMode::Success,
            },
            1_024,
        )
        .unwrap();
        assert_eq!(dispatches.len(), 1, "{checkpoint}");
        let recovered = ledger
            .load_turn(&before.facts[0].turn_id.clone().unwrap())
            .unwrap();
        for kind in [
            "effect.prepared",
            "safety.decided",
            "effect.authorized",
            "sandbox.bound",
            "sandbox.preflighted",
            "effect.started",
        ] {
            assert_eq!(
                recovered
                    .facts
                    .iter()
                    .filter(|fact| fact.kind.as_str() == kind)
                    .count(),
                1,
                "{checkpoint}:{kind}"
            );
        }
    }
}

#[test]
fn local_startup_commits_one_bound_interaction_without_replacement_dispatch() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("startup-interaction.sqlite3"));
    let prepared = v3_catalog()
        .prepare_v3(
            &ToolIntent::new("call", "read_file", r#"{"path":"a"}"#),
            &Resolver,
        )
        .unwrap();
    setup
        .ledger
        .commit(
            setup.session.clone(),
            setup.version,
            f0_recovery_facts(&setup, &prepared)[..1].to_vec(),
        )
        .unwrap();

    let dispatches = recover_local_dispatches_with_f0(
        &mut setup.ledger,
        3,
        timestamp(),
        &RecoveryFactory {
            decision: Decision::Interaction,
            mode: ExecutionMode::Success,
        },
        1_024,
    )
    .unwrap();

    let snapshot = setup.ledger.load_turn(&setup.turn).unwrap();
    assert!(dispatches.is_empty());
    for kind in [
        "interaction.requested",
        "execution.suspended",
        "turn.suspended",
    ] {
        assert_eq!(
            snapshot
                .facts
                .iter()
                .filter(|fact| fact.kind.as_str() == kind)
                .count(),
            1,
            "{kind}"
        );
    }
    assert!(!snapshot
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "effect.started"));
}

#[test]
fn process_loss_is_terminated_before_uncertainty_becomes_durable() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("process-loss.sqlite3"));
    let prepared = BuiltinT1Catalogue::new("policy-1", ["rust-toolchain"])
        .unwrap()
        .prepare(&ToolIntent::new(
            "call",
            T1_PROCESS_RUN,
            r#"{"lane":"rust-toolchain","argv":["cargo","test"],"working_directory":".","workspace_mode":"write","max_output_bytes":4096,"timeout_ms":30000}"#,
        ))
        .unwrap();
    setup
        .ledger
        .commit(
            setup.session.clone(),
            setup.version,
            process_started_facts(&setup, &prepared),
        )
        .unwrap();

    let failed_calls = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        recover_local_dispatches_with_f0(
            &mut setup.ledger,
            3,
            timestamp(),
            &LossRecoveryFactory {
                reconciliations: Arc::clone(&failed_calls),
                fail: true,
            },
            1_024,
        ),
        Err(LocalRecoveryError::F0RecoveryFailed)
    );
    assert_eq!(failed_calls.load(Ordering::SeqCst), 1);
    assert!(!setup
        .ledger
        .load_turn(&setup.turn)
        .unwrap()
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "effect.uncertain"));

    let successful_calls = Arc::new(AtomicUsize::new(0));
    let dispatches = recover_local_dispatches_with_f0(
        &mut setup.ledger,
        3,
        timestamp(),
        &LossRecoveryFactory {
            reconciliations: Arc::clone(&successful_calls),
            fail: false,
        },
        1_024,
    )
    .unwrap();
    assert!(dispatches.is_empty());
    assert_eq!(successful_calls.load(Ordering::SeqCst), 1);
    let snapshot = setup.ledger.load_turn(&setup.turn).unwrap();
    assert!(snapshot
        .facts
        .iter()
        .any(|fact| fact.kind.as_str() == "effect.uncertain"));
    assert_eq!(
        snapshot.facts.last().unwrap().kind.as_str(),
        "turn.suspended"
    );
}

fn process_started_facts(
    setup: &Setup,
    prepared: &garive_tools::PreparedToolCall,
) -> Vec<FactDraft> {
    let invocation = garive_tools::ToolInvocationId::new("f0-recovery").unwrap();
    let request = garive_runtime::SafetyRequestV1::new(
        child_id(&invocation, "safety-request"),
        invocation.clone(),
        prepared,
        "actor",
        None,
        None,
        "policy-1",
    )
    .unwrap();
    let decision = SafetyDecisionV1::new(
        "process-safety",
        SafetyDisposition::Allow,
        invocation.clone(),
        prepared.input_digest(),
        Some("a".repeat(64)),
        "policy-1",
        None,
    )
    .unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("process-grant").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "policy-1",
    )
    .unwrap();
    let mut facts = plan_f0_safety_decision(
        &F0SafetyDecisionContext {
            turn_id: setup.turn.clone(),
            execution_id: setup.execution.clone(),
            recorded_at: timestamp().into(),
        },
        &request,
        prepared,
        &decision,
    )
    .unwrap();
    let access = ToolAccessPolicyV1::new(
        "policy-1",
        [AccessPolicyEntry::new(".", [AccessMode::Read, AccessMode::Write]).unwrap()],
        [AccessPolicyEntry::new("rust-toolchain", [AccessMode::Exclusive]).unwrap()],
        [],
        [],
        2,
        2_097_152,
    )
    .unwrap();
    facts.extend(
        plan_f0_sandbox_admission(
            &F0EffectAdmissionContext {
                turn_id: setup.turn.clone(),
                execution_id: setup.execution.clone(),
                preflight_id: "process-preflight".into(),
                effective_limits_digest: "e".repeat(64),
                recorded_at: timestamp().into(),
            },
            &request,
            prepared,
            &grant,
            &decision,
            &SandboxBindingV1::new(
                "process-binding",
                "workspace",
                "garive.builtin.process",
                "process-v1",
                "policy-1",
                access,
                prepared.sandbox_requirements().unwrap().clone(),
            )
            .unwrap(),
            "process-dispatch-1",
        )
        .unwrap()
        .facts,
    );
    facts.push(FactDraft {
        fact_id: FactId::try_from("process-started").unwrap(),
        turn_id: Some(setup.turn.clone()),
        execution_id: Some(setup.execution.clone()),
        model_request_id: None,
        tool_invocation_id: Some(garive_ledger::ToolInvocationId::try_from("f0-recovery").unwrap()),
        kind: FactKind::new("effect.started").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({
            "prepared_digest":prepared.input_digest(),"grant_id":"process-grant",
            "executor_id":"garive.builtin.process","executor_revision":"process-v1",
            "dispatch_attempt_id":"process-dispatch-1"
        }))
        .unwrap(),
        recorded_at: timestamp().into(),
    });
    facts
}

fn f0_recovery_facts(
    Setup {
        turn, execution, ..
    }: &Setup,
    prepared: &garive_tools::PreparedToolCall,
) -> Vec<FactDraft> {
    let invocation = garive_tools::ToolInvocationId::new("f0-recovery").unwrap();
    let request = garive_runtime::SafetyRequestV1::new(
        child_id(&invocation, "safety-request"),
        invocation.clone(),
        prepared,
        "actor",
        None,
        None,
        "policy-1",
    )
    .unwrap();
    let decision = SafetyDecisionV1::new(
        "safety-decision",
        SafetyDisposition::Allow,
        invocation.clone(),
        prepared.input_digest(),
        Some("a".repeat(64)),
        "policy-1",
        None,
    )
    .unwrap();
    let grant = InvocationGrant::new(
        GrantId::new(child_id(&invocation, "grant")).unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "policy-1",
    )
    .unwrap();
    let mut facts = plan_f0_safety_decision(
        &F0SafetyDecisionContext {
            turn_id: turn.clone(),
            execution_id: execution.clone(),
            recorded_at: timestamp().into(),
        },
        &request,
        prepared,
        &decision,
    )
    .unwrap();
    facts.extend(
        plan_f0_sandbox_admission(
            &F0EffectAdmissionContext {
                turn_id: turn.clone(),
                execution_id: execution.clone(),
                preflight_id: "preflight".into(),
                effective_limits_digest: "e".repeat(64),
                recorded_at: timestamp().into(),
            },
            &request,
            prepared,
            &grant,
            &decision,
            &SandboxBindingV1::new(
                "binding",
                "workspace",
                "local.read",
                "1",
                "policy-1",
                access_policy(),
                sandbox_requirements(),
            )
            .unwrap(),
            "dispatch-1",
        )
        .unwrap()
        .facts,
    );
    facts
}

fn child_id(invocation: &garive_tools::ToolInvocationId, kind: &str) -> String {
    format!(
        "{kind}-{:x}",
        Sha256::digest(format!("{}:{kind}", invocation.as_str()).as_bytes())
    )
}

#[test]
fn interaction_uses_one_suspension_binding_from_request_through_terminal() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Interaction,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);
    let result = block_on(port.invoke(&request_id, &prepared)).unwrap();
    let version = port.session_version().unwrap();
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (0, 0));
    let binding = result.suspension_binding.clone().unwrap();
    let suspension_id = match &binding {
        GovernedSuspensionBinding::Interaction { suspension_id, .. } => suspension_id.clone(),
        _ => panic!("expected interaction binding"),
    };
    let report = ExecutionReport {
        outcome: AgentOutcome::Suspended {
            reason: SuspensionReason::ApprovalRequired,
            partial_items: vec![],
            last_durable_position: result.through_position,
            governed_binding: Some(binding),
        },
        completed_iterations: 1,
        usage: UsageSummary {
            input_tokens: TokenCount::Known(1),
            output_tokens: TokenCount::Known(1),
            estimated: false,
        },
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: setup.turn.clone(),
            execution_id: setup.execution.clone(),
            recorded_at: timestamp().into(),
        },
        &report,
    )
    .unwrap();
    setup
        .ledger
        .commit(setup.session.clone(), version, terminal)
        .unwrap();
    let facts = setup.ledger.load_turn(&setup.turn).unwrap().facts;
    let requested = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "interaction.requested")
        .unwrap();
    let suspended = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "turn.suspended")
        .unwrap();
    assert_eq!(payload(requested)["suspension_id"], suspension_id);
    assert_eq!(
        payload(requested)["response_schema"]["inline_utf8"],
        r#"{"type":"boolean"}"#
    );
    assert_eq!(payload(suspended)["suspension_id"], suspension_id);
    let state = reconstruct_suspended_turn(&setup.ledger.load_turn(&setup.turn).unwrap()).unwrap();
    assert_eq!(
        state.interaction.as_ref().unwrap().response_schema,
        json!({"type":"boolean"})
    );
    let command = |input: &str| ContinueTurnCommand {
        command_id: RuntimeCommandId::new(format!("continue-{input}")).unwrap(),
        session_id: setup.session.clone(),
        turn_id: setup.turn.clone(),
        expected_suspension_id: state.suspension_id.clone(),
        expected_session_version: state.session_version,
        continuation_input: ContinuationInput::InteractionResponse {
            canonical_json: input.into(),
            representation: InteractionInputRepresentation::JsonField,
        },
        interaction: state.interaction.clone(),
        recorded_at: timestamp().into(),
    };
    assert!(garive_runtime::plan_continue_turn(&command("true"), &state).is_ok());
    assert_eq!(
        garive_runtime::plan_continue_turn(&command("\"not boolean\""), &state),
        Err(RuntimeCommandError::InvalidCommand)
    );
    assert_eq!(
        garive_runtime::plan_continue_turn(&command(" true"), &state),
        Err(RuntimeCommandError::InvalidCommand)
    );
}

#[test]
fn invalid_executor_receipt_becomes_durable_uncertainty_without_observation() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::InvalidReceipt,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);
    let result = block_on(port.invoke(&request_id, &prepared)).unwrap();
    assert!(matches!(
        result.result,
        garive_tools::GovernedToolResult::Suspend(_)
    ));
    assert!(matches!(
        result.suspension_binding,
        Some(GovernedSuspensionBinding::OperatorReconciliation { .. })
    ));
    drop(port);
    assert_tail(
        &setup,
        &[
            "effect.prepared",
            "effect.authorized",
            "effect.started",
            "effect.uncertain",
        ],
    );
}

#[test]
fn unsupported_requirement_fails_before_started_and_never_dispatches() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Unsupported,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);
    let result = block_on(port.invoke(&request_id, &prepared)).unwrap();
    assert!(matches!(
        result.result,
        garive_tools::GovernedToolResult::Fail(_)
    ));
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (1, 0));
    assert_tail(
        &setup,
        &["effect.prepared", "effect.authorized", "effect.failed"],
    );
}

#[test]
fn denial_commits_observation_without_executor_access() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Deny,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let request_id = setup.request_id.clone();
    let prepared = setup.prepared.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);
    let result = block_on(port.invoke(&request_id, &prepared)).unwrap();
    assert!(matches!(
        result.result,
        garive_tools::GovernedToolResult::Observation(_)
    ));
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (0, 0));
    assert_tail(
        &setup,
        &["effect.prepared", "effect.denied", "effect.observation"],
    );
}

#[test]
fn preparation_rejection_commits_source_model_binding_without_invocation() {
    let directory = tempdir().unwrap();
    let mut setup = setup(&directory.path().join("ledger.sqlite3"));
    let mut authority = Authority {
        decision: Decision::Approve,
    };
    let mut executor = Executor {
        mode: ExecutionMode::Success,
        prepares: 0,
        dispatches: 0,
    };
    let intent = ToolIntent::new("call", "missing", "{}");
    let error = tool_catalog().prepare(&intent).unwrap_err();
    let request_id = setup.request_id.clone();
    let mut port = port(&mut setup, &mut authority, &mut executor);
    let result = block_on(port.reject(&request_id, &intent, &error)).unwrap();
    assert!(matches!(
        result.result,
        garive_tools::GovernedToolResult::Observation(_)
    ));
    drop(port);
    assert_eq!((executor.prepares, executor.dispatches), (0, 0));
    assert_tail(&setup, &["tool.preparation_rejected"]);
}

fn port<'a>(
    setup: &'a mut Setup,
    authority: &'a mut Authority,
    executor: &'a mut Executor,
) -> SqliteGovernedEffectPort<'a> {
    SqliteGovernedEffectPort::new(
        &mut setup.ledger,
        authority,
        executor,
        GovernedEffectConfig {
            session_id: setup.session.clone(),
            expected_session_version: setup.version,
            initial_through_position: setup.position,
            turn_id: setup.turn.clone(),
            execution_id: setup.execution.clone(),
            recorded_at: timestamp().into(),
        },
    )
    .unwrap()
}

fn assert_tail(setup: &Setup, expected: &[&str]) {
    let facts = setup.ledger.load_turn(&setup.turn).unwrap().facts;
    let actual = facts
        .iter()
        .rev()
        .take(expected.len())
        .map(|fact| fact.kind.as_str())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn model_request() -> ModelRequest {
    ModelRequest {
        request_id: ModelRequestId::new("request"),
        target_id: ModelTargetId::new("target"),
        required_capabilities: vec![ModelCapability::Tools],
        input_items: vec![],
        tools: vec![ToolDescriptor {
            name: "read_file".into(),
            description: "Read one file.".into(),
            definition_revision: "1".into(),
            input_schema_json: r#"{"type":"object"}"#.into(),
            strict: true,
        }],
        output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        trace_metadata: vec![],
    }
}

fn tool_catalog() -> ToolCatalog {
    ToolCatalog::new([ToolDefinition::new(
        "read_file",
        "1",
        "Read one file.",
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 1_024).unwrap(),
        ReplayClass::ReadOnly,
    )
    .unwrap()])
    .unwrap()
}

fn v3_catalog() -> ToolCatalog {
    ToolCatalog::new([v3_definition()]).unwrap()
}

fn v3_definition() -> ToolDefinition {
    ToolDefinition::new_v3(
        "read_file",
        "1",
        "Read one file.",
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 1_024).unwrap(),
        ReplayClass::ReadOnly,
        access_policy(),
        "resolver-1",
        sandbox_requirements(),
    )
    .unwrap()
}

fn access_policy() -> ToolAccessPolicyV1 {
    ToolAccessPolicyV1::new(
        "access-1",
        [AccessPolicyEntry::new("a", [AccessMode::Read]).unwrap()],
        [],
        [],
        [],
        1,
        1_024,
    )
    .unwrap()
}

fn sandbox_requirements() -> SandboxRequirementsV1 {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemRead],
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

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

fn draft(
    id: &str,
    kind: &str,
    turn: Option<&TurnId>,
    execution: Option<&ExecutionId>,
    payload: Value,
) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: turn.cloned(),
        execution_id: execution.cloned(),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: timestamp().into(),
    }
}

fn runtime_payload(kind: &str) -> Value {
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/ledger/runtime-facts-v1.json"),
        )
        .unwrap(),
    )
    .unwrap();
    fixture["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["kind"] == kind)
        .unwrap()["payload"]
        .clone()
}

fn payload(fact: &garive_ledger::DurableFact) -> Value {
    serde_json::from_str(fact.payload.as_json()).unwrap()
}

fn timestamp() -> &'static str {
    "2026-08-29T00:00:00Z"
}
