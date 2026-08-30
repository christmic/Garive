use futures::executor::block_on;
use garive_core::{
    AgentOutcome, ExecutionReport, GovernedEffectPort, GovernedSuspensionBinding, SuspensionReason,
    UsageSummary,
};
use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, SessionId, TurnId,
};
use garive_llm::{
    InvokeOutcome, ModelCapability, ModelItem, ModelOutputSettings, ModelRequest, ModelRequestId,
    ModelStopReason, ModelTargetId, ModelUsage, TextMode, TokenCount, ToolDescriptor, UsageSource,
};
use garive_runtime::{
    plan_core_terminal, plan_model_prepared, plan_model_started, plan_model_terminal,
    reconstruct_suspended_turn, AuthorityDecision, AuthorityFuture, AuthorityPort,
    AuthorityRequest, ContinuationInput, ContinueTurnCommand, CoreTerminalContext,
    ExecutorDispatch, ExecutorFuture, ExecutorPort, GovernedEffectConfig, ModelLifecycleContext,
    PreparedExecution, RuntimeCommandError, RuntimeCommandId, SqliteGovernedEffectPort,
    SqliteLedger,
};
use garive_tools::{
    EffectReceipt, ExecutionCapability, ExecutionFact, ExecutionRequirements, InteractionKind,
    ReceiptId, ReplayClass, TerminalClassification, ToolCatalog, ToolDefinition, ToolIntent,
};
use serde_json::{json, Value};
use tempfile::tempdir;

enum Decision {
    Approve,
    Interaction,
    Deny,
}

struct Authority {
    decision: Decision,
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

enum ExecutionMode {
    Success,
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
                ExecutionMode::Success | ExecutionMode::Unsupported => command.receipt_id,
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
}

struct Setup {
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
        draft("f1", "session.opened", None, None, json!({})),
        draft(
            "f2",
            "turn.started",
            Some(&turn),
            None,
            runtime_payload("turn.started"),
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
        state.interaction.as_ref().unwrap().response_schema.as_ref(),
        Some(&json!({"type":"boolean"}))
    );
    let command = |input: &str| ContinueTurnCommand {
        command_id: RuntimeCommandId::new(format!("continue-{input}")).unwrap(),
        session_id: setup.session.clone(),
        turn_id: setup.turn.clone(),
        expected_suspension_id: state.suspension_id.clone(),
        expected_session_version: state.session_version,
        continuation_input: ContinuationInput::ExternalInput(input.into()),
        interaction: state.interaction.clone(),
        recorded_at: timestamp().into(),
    };
    assert!(garive_runtime::plan_continue_turn(&command("true"), &state).is_ok());
    assert_eq!(
        garive_runtime::plan_continue_turn(&command("\"not boolean\""), &state),
        Err(RuntimeCommandError::ContinuationMismatch)
    );
    assert_eq!(
        garive_runtime::plan_continue_turn(&command(" true"), &state),
        Err(RuntimeCommandError::ContinuationMismatch)
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
