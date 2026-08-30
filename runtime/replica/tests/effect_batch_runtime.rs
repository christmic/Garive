use std::{
    collections::HashSet,
    future::pending,
    sync::{Arc, Mutex},
    time::Duration,
};

use garive_ledger::{
    CanonicalPayload, ExecutionId, FactDraft, FactId, FactKind, SessionId, TurnId,
};
#[cfg(unix)]
use garive_runtime::ConfinedFileReadExecutor;
use garive_runtime::{
    plan_effect_batch_admission, reconstruct_effect_batch_recovery, AuthorizedBatchInvocation,
    BatchRuntimeError, BatchTerminal, CancellationEvidence, ConcurrentExecutorDispatch,
    ConcurrentExecutorPort, EffectBatchAdmissionContext, EffectBatchDispatcher,
    EffectBatchMemberRecovery, EffectBatchPublisher, EffectBatchRuntimeLimits, EffectCancellation,
    ExecutorDispatchError, PreparedExecution, SqliteEffectBatchPublisher, SqliteLedger,
};
use garive_tools::{
    plan_effect_batch, AccessMode, AccessNamespace, AccessPolicyEntry, EffectBatchLimitsV1,
    EffectReceipt, ExecutionCapability, ExecutionFact, ExecutionRequirements, GrantId,
    InvocationAccessSet, InvocationGrant, ReceiptId, ReplayClass, ResourceAccess,
    TerminalClassification, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition,
    ToolIntent, ToolInvocationId,
};
use serde_json::{json, Value};
use tempfile::tempdir;

struct Resolver;

impl ToolAccessResolver for Resolver {
    fn revision(&self) -> &str {
        "resolver-v1"
    }

    fn resolve(
        &self,
        arguments: &Value,
    ) -> Result<InvocationAccessSet, garive_tools::PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap(),
            AccessMode::Read,
        )?])
    }
}

fn prepared(index: usize) -> garive_tools::PreparedToolCall {
    prepared_path(index, &format!("src/{index}"))
}

fn prepared_path(index: usize, path: &str) -> garive_tools::PreparedToolCall {
    let definition = ToolDefinition::new_v2(
        format!("read_{index}"),
        "revision-v2",
        "read fixture",
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 100, 512).unwrap(),
        ReplayClass::ReadOnly,
        ToolAccessPolicyV1::new(
            "policy-v1",
            [AccessPolicyEntry::new("src", [AccessMode::Read]).unwrap()],
            [],
            [],
            [],
            1,
            512,
        )
        .unwrap(),
        "resolver-v1",
    )
    .unwrap();
    ToolCatalog::new([definition])
        .unwrap()
        .prepare_v2(
            &ToolIntent::new(
                format!("call-{index}"),
                format!("read_{index}"),
                format!(r#"{{"path":"{path}"}}"#),
            ),
            &Resolver,
        )
        .unwrap()
}

fn invocations(count: usize) -> Vec<AuthorizedBatchInvocation> {
    (0..count)
        .map(|index| authorized(prepared(index), index))
        .collect()
}

fn authorized(prepared: garive_tools::PreparedToolCall, index: usize) -> AuthorizedBatchInvocation {
    let invocation_id = ToolInvocationId::new(format!("invocation-{index}")).unwrap();
    let grant = InvocationGrant::new(
        GrantId::new(format!("grant-{index}")).unwrap(),
        invocation_id.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "authority-v1",
    )
    .unwrap();
    AuthorizedBatchInvocation {
        invocation_id,
        prepared,
        grant,
        receipt_id: format!("receipt-{index}"),
    }
}

fn limits(timeout_ms: u64) -> EffectBatchRuntimeLimits {
    EffectBatchRuntimeLimits {
        max_parallel_reads: 3,
        queue_timeout: Duration::from_millis(100),
        invocation_timeout: Duration::from_millis(timeout_ms),
        cancellation_grace: Duration::from_millis(2),
    }
}

struct Publisher {
    events: Vec<String>,
    started: Arc<Mutex<HashSet<String>>>,
    fail_terminal: Option<usize>,
}

impl EffectBatchPublisher for Publisher {
    fn commit_started(
        &mut self,
        index: usize,
        invocation: &AuthorizedBatchInvocation,
        _: &PreparedExecution,
    ) -> Result<(), BatchRuntimeError> {
        self.events.push(format!("start:{index}"));
        self.started
            .lock()
            .unwrap()
            .insert(invocation.invocation_id.as_str().into());
        Ok(())
    }

    fn publish_terminal(
        &mut self,
        index: usize,
        _: &AuthorizedBatchInvocation,
        _: &PreparedExecution,
        _: &BatchTerminal,
    ) -> Result<(), BatchRuntimeError> {
        self.events.push(format!("terminal:{index}"));
        if self.fail_terminal == Some(index) {
            Err(BatchRuntimeError::DurabilityFailure)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
enum Mode {
    Complete(Vec<u64>),
    Pending(CancellationEvidence),
    Oversized,
    InvalidReceipt,
}

struct Executor {
    mode: Mode,
    started: Arc<Mutex<HashSet<String>>>,
}

impl ConcurrentExecutorPort for Executor {
    fn prepare(&self, invocation: &AuthorizedBatchInvocation) -> Result<PreparedExecution, String> {
        Ok(PreparedExecution {
            executor_id: "confined-read".into(),
            executor_revision: "1".into(),
            dispatch_attempt_id: format!("attempt-{}", invocation.invocation_id.as_str()),
        })
    }

    fn dispatch<'a>(
        &'a self,
        command: ConcurrentExecutorDispatch,
        _: EffectCancellation,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<ExecutionFact, ExecutorDispatchError>>
                + Send
                + 'a,
        >,
    > {
        assert!(self
            .started
            .lock()
            .unwrap()
            .contains(command.invocation_id.as_str()));
        Box::pin(async move {
            let content = match &self.mode {
                Mode::Pending(_) => return pending().await,
                Mode::Oversized => json!({"value":"x".repeat(600)}),
                Mode::InvalidReceipt => json!({"value":"x"}),
                Mode::Complete(delays) => {
                    let index = command
                        .invocation_id
                        .as_str()
                        .strip_prefix("invocation-")
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis(delays[index])).await;
                    json!({"index":index})
                }
            };
            let digest = CanonicalPayload::from_value(&content)
                .unwrap()
                .sha256()
                .to_owned();
            Ok(ExecutionFact::Completed {
                receipt: Some(EffectReceipt {
                    receipt_id: ReceiptId::new(if matches!(&self.mode, Mode::InvalidReceipt) {
                        "wrong-receipt".into()
                    } else {
                        command.receipt_id
                    })
                    .unwrap(),
                    invocation_id: command.invocation_id,
                    prepared_digest: command.prepared.input_digest().into(),
                    grant_id: command.grant.grant_id,
                    executor_id: command.execution.executor_id,
                    executor_revision: command.execution.executor_revision,
                    terminal_classification: TerminalClassification::Completed,
                    result_digest: digest,
                }),
                content,
                truncated: false,
            })
        })
    }

    fn cancellation_evidence(&self, _: &ToolInvocationId) -> CancellationEvidence {
        match &self.mode {
            Mode::Pending(value) => value.clone(),
            _ => CancellationEvidence::Unknown,
        }
    }
}

async fn run(mode: Mode, cancellation: EffectCancellation) -> (Publisher, Vec<BatchTerminal>) {
    let invocations = invocations(3);
    let plan = plan_effect_batch(
        &invocations
            .iter()
            .map(|value| value.prepared.clone())
            .collect::<Vec<_>>(),
        &EffectBatchLimitsV1::new(3, 1, 3, 3, 1536).unwrap(),
    )
    .unwrap();
    let started = Arc::new(Mutex::new(HashSet::new()));
    let executor = Executor {
        mode,
        started: started.clone(),
    };
    let mut publisher = Publisher {
        events: Vec::new(),
        started,
        fail_terminal: None,
    };
    let report = EffectBatchDispatcher::new(limits(30))
        .unwrap()
        .execute(
            &plan,
            &invocations,
            limits(30),
            &cancellation,
            &executor,
            &mut publisher,
        )
        .await
        .unwrap();
    (
        publisher,
        report
            .terminals
            .into_iter()
            .map(|(_, value)| value)
            .collect(),
    )
}

#[tokio::test]
async fn completion_permutations_publish_identical_model_order() {
    for delays in [
        vec![1, 3, 5],
        vec![1, 5, 3],
        vec![3, 1, 5],
        vec![3, 5, 1],
        vec![5, 1, 3],
        vec![5, 3, 1],
    ] {
        let (publisher, terminals) =
            run(Mode::Complete(delays), EffectCancellation::default()).await;
        assert_eq!(
            publisher.events,
            [
                "start:0",
                "start:1",
                "start:2",
                "terminal:0",
                "terminal:1",
                "terminal:2"
            ]
        );
        assert!(terminals
            .iter()
            .all(|value| matches!(value, BatchTerminal::Execution(_))));
    }
}

#[tokio::test]
async fn timeout_cancellation_uncertainty_and_result_bounds_are_explicit() {
    let (_, timed_out) = run(
        Mode::Pending(CancellationEvidence::ProvenNotCompleted),
        EffectCancellation::default(),
    )
    .await;
    assert!(timed_out
        .iter()
        .all(|value| *value == BatchTerminal::ExecutionTimedOut));

    let cancellation = EffectCancellation::default();
    let signal = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1)).await;
        signal.cancel();
    });
    let (_, cancelled) = run(
        Mode::Pending(CancellationEvidence::ProvenNotCompleted),
        cancellation,
    )
    .await;
    assert!(cancelled
        .iter()
        .all(|value| *value == BatchTerminal::Cancelled));

    let (_, uncertain) = run(
        Mode::Pending(CancellationEvidence::Unknown),
        EffectCancellation::default(),
    )
    .await;
    assert!(uncertain
        .iter()
        .all(|value| *value == BatchTerminal::Uncertain));
    let (_, invalid_receipt) = run(Mode::InvalidReceipt, EffectCancellation::default()).await;
    assert!(invalid_receipt
        .iter()
        .all(|value| *value == BatchTerminal::Uncertain));
    let (_, oversized) = run(Mode::Oversized, EffectCancellation::default()).await;
    assert!(oversized
        .iter()
        .all(|value| *value == BatchTerminal::ResultBoundExceeded));
}

#[tokio::test]
async fn terminal_durability_failure_stops_later_publication() {
    let invocations = invocations(3);
    let plan = plan_effect_batch(
        &invocations
            .iter()
            .map(|value| value.prepared.clone())
            .collect::<Vec<_>>(),
        &EffectBatchLimitsV1::new(3, 1, 3, 3, 1536).unwrap(),
    )
    .unwrap();
    let started = Arc::new(Mutex::new(HashSet::new()));
    let executor = Executor {
        mode: Mode::Complete(vec![3, 2, 1]),
        started: started.clone(),
    };
    let mut publisher = Publisher {
        events: Vec::new(),
        started,
        fail_terminal: Some(1),
    };
    let error = EffectBatchDispatcher::new(limits(30))
        .unwrap()
        .execute(
            &plan,
            &invocations,
            limits(30),
            &EffectCancellation::default(),
            &executor,
            &mut publisher,
        )
        .await
        .unwrap_err();
    assert_eq!(error, BatchRuntimeError::DurabilityFailure);
    assert_eq!(
        publisher.events,
        ["start:0", "start:1", "start:2", "terminal:0", "terminal:1"]
    );
}

#[tokio::test]
async fn shared_capacity_has_a_separate_bounded_queue_timeout() {
    let runtime_limits = EffectBatchRuntimeLimits {
        max_parallel_reads: 1,
        queue_timeout: Duration::from_millis(100),
        invocation_timeout: Duration::from_millis(100),
        cancellation_grace: Duration::from_millis(2),
    };
    let dispatcher = Arc::new(EffectBatchDispatcher::new(runtime_limits).unwrap());
    let first_invocations = invocations(1);
    let first_plan = plan_effect_batch(
        &[first_invocations[0].prepared.clone()],
        &EffectBatchLimitsV1::new(1, 1, 1, 1, 512).unwrap(),
    )
    .unwrap();
    let first_started = Arc::new(Mutex::new(HashSet::new()));
    let first_signal = first_started.clone();
    let first_dispatcher = dispatcher.clone();
    let first = tokio::spawn(async move {
        let executor = Executor {
            mode: Mode::Complete(vec![40]),
            started: first_started.clone(),
        };
        let mut publisher = Publisher {
            events: Vec::new(),
            started: first_started,
            fail_terminal: None,
        };
        let cancellation = EffectCancellation::default();
        first_dispatcher
            .execute(
                &first_plan,
                &first_invocations,
                runtime_limits,
                &cancellation,
                &executor,
                &mut publisher,
            )
            .await
            .unwrap();
    });
    tokio::time::timeout(Duration::from_millis(50), async {
        while first_signal.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let second_invocations = invocations(1);
    let second_plan = plan_effect_batch(
        &[second_invocations[0].prepared.clone()],
        &EffectBatchLimitsV1::new(1, 1, 1, 1, 512).unwrap(),
    )
    .unwrap();
    let second_started = Arc::new(Mutex::new(HashSet::new()));
    let executor = Executor {
        mode: Mode::Complete(vec![1]),
        started: second_started.clone(),
    };
    let mut publisher = Publisher {
        events: Vec::new(),
        started: second_started,
        fail_terminal: None,
    };
    let short_queue = EffectBatchRuntimeLimits {
        queue_timeout: Duration::from_millis(2),
        ..runtime_limits
    };
    let error = dispatcher
        .execute(
            &second_plan,
            &second_invocations,
            short_queue,
            &EffectCancellation::default(),
            &executor,
            &mut publisher,
        )
        .await
        .unwrap_err();
    assert_eq!(error, BatchRuntimeError::QueueTimeout);
    assert!(publisher.events.is_empty());
    first.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn confined_executor_rejects_symlinks_and_non_exact_case() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let root = directory.path().join("workspace");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/inside.txt"), "inside").unwrap();
    std::fs::write(root.join("src/Exact.txt"), "case").unwrap();
    std::fs::write(directory.path().join("outside.txt"), "secret").unwrap();
    symlink("../../outside.txt", root.join("src/link.txt")).unwrap();
    let executor = ConfinedFileReadExecutor::new(&root, "confined-v1").unwrap();

    let inside = confined_dispatch(
        &executor,
        authorized(prepared_path(10, "src/inside.txt"), 10),
    )
    .await;
    assert!(matches!(
        inside,
        ExecutionFact::Completed { content, .. } if content == json!({"text":"inside"})
    ));

    for invocation in [
        authorized(prepared_path(11, "src/link.txt"), 11),
        authorized(prepared_path(12, "src/exact.txt"), 12),
    ] {
        assert!(matches!(
            confined_dispatch(&executor, invocation).await,
            ExecutionFact::Failed { code, .. } if code == "access_denied"
        ));
    }
}

#[cfg(unix)]
async fn confined_dispatch(
    executor: &ConfinedFileReadExecutor,
    invocation: AuthorizedBatchInvocation,
) -> ExecutionFact {
    let execution = executor.prepare(&invocation).unwrap();
    executor
        .dispatch(
            ConcurrentExecutorDispatch {
                invocation_id: invocation.invocation_id,
                prepared: invocation.prepared,
                grant: invocation.grant,
                execution,
                receipt_id: invocation.receipt_id,
            },
            EffectCancellation::default(),
        )
        .await
        .unwrap()
}

#[test]
fn admission_facts_commit_prepared_v2_authorizations_then_exact_plan() {
    let directory = tempdir().unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("ledger.db")).unwrap();
    let session = SessionId::try_from("session").unwrap();
    let turn = TurnId::try_from("turn").unwrap();
    let execution = ExecutionId::try_from("execution").unwrap();
    let initial = ledger.commit(
        session.clone(),
        0,
        vec![
            raw_fact("open", "session.opened", None, None, json!({})),
            raw_fact("turn", "turn.started", Some(&turn), None, json!({
                "command_id":"command","kind":"start","agent_instance_id":"agent",
                "definition_id":"definition","definition_revision":"revision",
                "snapshot_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "trusted_input_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            })),
            raw_fact("execution", "execution.started", Some(&turn), Some(&execution), json!({
                "snapshot_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "through_position":0,"completed_iterations":0,"limits":{"max_iterations":1},"recovery_ordinal":0
            })),
        ],
    ).unwrap();
    let invocations = invocations(3);
    let plan = plan_effect_batch(
        &invocations
            .iter()
            .map(|value| value.prepared.clone())
            .collect::<Vec<_>>(),
        &EffectBatchLimitsV1::new(3, 1, 3, 3, 1536).unwrap(),
    )
    .unwrap();
    let context = EffectBatchAdmissionContext {
        turn_id: turn,
        execution_id: execution,
        max_parallel_reads: 3,
        max_buffered_result_bytes: 1536,
        recorded_at: "2026-08-30T00:00:00Z".into(),
    };
    let admission = plan_effect_batch_admission(&context, &plan, &invocations).unwrap();
    assert_eq!(admission.facts.len(), 7);
    assert_eq!(admission.facts[0].schema_version, 2);
    assert_eq!(
        admission.facts[6].kind.as_str(),
        "execution.effect_batch_planned"
    );
    let admitted = ledger
        .commit(session.clone(), initial.session_version, admission.facts)
        .unwrap();

    let mut stale = invocations.clone();
    stale[0].grant.prepared_digest = "b".repeat(64);
    assert_eq!(
        plan_effect_batch_admission(&context, &plan, &stale).unwrap_err(),
        BatchRuntimeError::InvalidBinding,
    );

    let mut publisher = SqliteEffectBatchPublisher::new(
        &mut ledger,
        session,
        admitted.session_version,
        context.clone(),
        plan.plan_digest(),
    )
    .unwrap();
    for (index, invocation) in invocations.iter().take(2).enumerate() {
        EffectBatchPublisher::commit_started(
            &mut publisher,
            index,
            invocation,
            &PreparedExecution {
                executor_id: "confined-read".into(),
                executor_revision: "1".into(),
                dispatch_attempt_id: format!("partial-{index}"),
            },
        )
        .unwrap();
    }
    drop(publisher);
    let recovered = reconstruct_effect_batch_recovery(
        &ledger,
        &context.turn_id,
        &context.execution_id,
        plan.plan_digest(),
        &invocations,
    )
    .unwrap();
    assert_eq!(
        recovered.members,
        [
            EffectBatchMemberRecovery::NeedsReconciliation,
            EffectBatchMemberRecovery::NeedsReconciliation,
            EffectBatchMemberRecovery::Authorized,
        ]
    );
}

struct SqlExecutor {
    path: std::path::PathBuf,
    turn: TurnId,
}

impl ConcurrentExecutorPort for SqlExecutor {
    fn prepare(&self, invocation: &AuthorizedBatchInvocation) -> Result<PreparedExecution, String> {
        Ok(PreparedExecution {
            executor_id: "confined-read".into(),
            executor_revision: "1".into(),
            dispatch_attempt_id: format!("attempt-{}", invocation.invocation_id.as_str()),
        })
    }

    fn dispatch<'a>(
        &'a self,
        command: ConcurrentExecutorDispatch,
        _: EffectCancellation,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<ExecutionFact, ExecutorDispatchError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let ledger = SqliteLedger::open(&self.path).unwrap();
            let snapshot = ledger.load_turn(&self.turn).unwrap();
            assert!(snapshot.facts.iter().any(|fact| {
                fact.kind.as_str() == "effect.started"
                    && fact.tool_invocation_id.as_ref().map(|id| id.as_str())
                        == Some(command.invocation_id.as_str())
            }));
            let content = json!({"tool":command.prepared.tool_name()});
            let result_digest = CanonicalPayload::from_value(&content)
                .unwrap()
                .sha256()
                .to_owned();
            Ok(ExecutionFact::Completed {
                receipt: Some(EffectReceipt {
                    receipt_id: ReceiptId::new(command.receipt_id).unwrap(),
                    invocation_id: command.invocation_id,
                    prepared_digest: command.prepared.input_digest().into(),
                    grant_id: command.grant.grant_id,
                    executor_id: command.execution.executor_id,
                    executor_revision: command.execution.executor_revision,
                    terminal_classification: TerminalClassification::Completed,
                    result_digest,
                }),
                content,
                truncated: false,
            })
        })
    }

    fn cancellation_evidence(&self, _: &ToolInvocationId) -> CancellationEvidence {
        CancellationEvidence::Unknown
    }
}

#[tokio::test]
async fn sqlite_publisher_commits_started_before_dispatch_and_ordered_observations() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("batch.db");
    let mut ledger = SqliteLedger::open(&path).unwrap();
    let session = SessionId::try_from("session-sql").unwrap();
    let turn = TurnId::try_from("turn-sql").unwrap();
    let execution = ExecutionId::try_from("execution-sql").unwrap();
    let initial = ledger
        .commit(
            session.clone(),
            0,
            vec![
                raw_fact("open-sql", "session.opened", None, None, json!({})),
                raw_fact(
                    "turn-sql",
                    "turn.started",
                    Some(&turn),
                    None,
                    turn_payload(),
                ),
                raw_fact(
                    "execution-sql",
                    "execution.started",
                    Some(&turn),
                    Some(&execution),
                    execution_payload(),
                ),
            ],
        )
        .unwrap();
    let invocations = invocations(3);
    let plan = plan_effect_batch(
        &invocations
            .iter()
            .map(|value| value.prepared.clone())
            .collect::<Vec<_>>(),
        &EffectBatchLimitsV1::new(3, 1, 3, 3, 1536).unwrap(),
    )
    .unwrap();
    let context = EffectBatchAdmissionContext {
        turn_id: turn.clone(),
        execution_id: execution,
        max_parallel_reads: 3,
        max_buffered_result_bytes: 1536,
        recorded_at: "2026-08-30T00:00:00Z".into(),
    };
    let admission = plan_effect_batch_admission(&context, &plan, &invocations).unwrap();
    let admitted = ledger
        .commit(session.clone(), initial.session_version, admission.facts)
        .unwrap();
    let recovered = reconstruct_effect_batch_recovery(
        &ledger,
        &turn,
        &context.execution_id,
        plan.plan_digest(),
        &invocations,
    )
    .unwrap();
    assert!(recovered
        .members
        .iter()
        .all(|state| *state == EffectBatchMemberRecovery::Authorized));
    let executor = SqlExecutor {
        path,
        turn: turn.clone(),
    };
    let mut publisher = SqliteEffectBatchPublisher::new(
        &mut ledger,
        session,
        admitted.session_version,
        context.clone(),
        plan.plan_digest(),
    )
    .unwrap();
    EffectBatchDispatcher::new(limits(30))
        .unwrap()
        .execute(
            &plan,
            &invocations,
            limits(30),
            &EffectCancellation::default(),
            &executor,
            &mut publisher,
        )
        .await
        .unwrap();
    drop(publisher);
    let recovered = reconstruct_effect_batch_recovery(
        &ledger,
        &turn,
        &context.execution_id,
        plan.plan_digest(),
        &invocations,
    )
    .unwrap();
    assert!(recovered
        .members
        .iter()
        .all(|state| *state == EffectBatchMemberRecovery::Observed));
    let kinds: Vec<_> = ledger
        .load_turn(&turn)
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect();
    assert_eq!(
        &kinds[9..],
        [
            "effect.started",
            "effect.started",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
        ]
    );
}

fn turn_payload() -> Value {
    json!({
        "command_id":"command","kind":"start","agent_instance_id":"agent",
        "definition_id":"definition","definition_revision":"revision",
        "snapshot_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "trusted_input_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    })
}

fn execution_payload() -> Value {
    json!({
        "snapshot_digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "through_position":0,"completed_iterations":0,"limits":{"max_iterations":1},"recovery_ordinal":0
    })
}

fn raw_fact(
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
        recorded_at: "2026-08-30T00:00:00Z".into(),
    }
}
