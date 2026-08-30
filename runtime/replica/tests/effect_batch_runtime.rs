use std::{
    collections::HashSet,
    future::pending,
    sync::{Arc, Mutex},
    time::Duration,
};

use garive_ledger::CanonicalPayload;
use garive_runtime::{
    AuthorizedBatchInvocation, BatchRuntimeError, BatchTerminal, CancellationEvidence,
    ConcurrentExecutorDispatch, ConcurrentExecutorPort, EffectBatchDispatcher,
    EffectBatchPublisher, EffectBatchRuntimeLimits, EffectCancellation, ExecutorDispatchError,
    PreparedExecution,
};
use garive_tools::{
    plan_effect_batch, AccessMode, AccessNamespace, AccessPolicyEntry, EffectBatchLimitsV1,
    EffectReceipt, ExecutionCapability, ExecutionFact, ExecutionRequirements, GrantId,
    InvocationAccessSet, InvocationGrant, ReceiptId, ReplayClass, ResourceAccess,
    TerminalClassification, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition,
    ToolIntent, ToolInvocationId,
};
use serde_json::{json, Value};

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
                format!(r#"{{"path":"src/{index}"}}"#),
            ),
            &Resolver,
        )
        .unwrap()
}

fn invocations(count: usize) -> Vec<AuthorizedBatchInvocation> {
    (0..count)
        .map(|index| {
            let prepared = prepared(index);
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
        })
        .collect()
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
                    receipt_id: ReceiptId::new(command.receipt_id).unwrap(),
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
