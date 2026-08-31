use std::sync::{Arc, Mutex};

use garive_runtime::{
    ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, ExecutorRecoveryRequest,
    ExecutorRoute, PreparedExecution, RoutedExecutorPort,
};
use garive_tools::{
    BuiltinT1Catalogue, EffectReceipt, GrantId, InvocationGrant, PreparedToolCall, ToolIntent,
    ToolInvocationId, T1_PROCESS_RUN, T1_READ_TEXT,
};

struct RecordingExecutor {
    returned_id: String,
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingExecutor {
    fn new(returned_id: &str, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            returned_id: returned_id.into(),
            events,
        }
    }
}

impl ExecutorPort for RecordingExecutor {
    fn prepare(
        &mut self,
        _: &ToolInvocationId,
        prepared: &PreparedToolCall,
        _: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        self.events
            .lock()
            .unwrap()
            .push(format!("prepare:{}", prepared.tool_name()));
        Ok(PreparedExecution {
            executor_id: self.returned_id.clone(),
            executor_revision: "executor.v1".into(),
            dispatch_attempt_id: "attempt-1".into(),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        self.events
            .lock()
            .unwrap()
            .push(format!("dispatch:{}", command.execution.executor_id));
        Box::pin(async { Err(ExecutorDispatchError::StartedWithoutReceipt) })
    }

    fn acknowledge_receipt(
        &mut self,
        _: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        self.events
            .lock()
            .unwrap()
            .push(format!("ack:{}", receipt.executor_id));
        Ok(())
    }

    fn reconcile_started_loss(
        &mut self,
        request: ExecutorRecoveryRequest<'_>,
    ) -> Result<(), ExecutorDispatchError> {
        self.events
            .lock()
            .unwrap()
            .push(format!("recover:{}", request.executor_id));
        Ok(())
    }
}

#[tokio::test]
async fn routes_prepare_dispatch_and_recovery_by_exact_frozen_bindings() {
    let workspace_events = Arc::new(Mutex::new(Vec::new()));
    let process_events = Arc::new(Mutex::new(Vec::new()));
    let mut router = RoutedExecutorPort::new([
        route("workspace", [T1_READ_TEXT], Arc::clone(&workspace_events)),
        route("process", [T1_PROCESS_RUN], Arc::clone(&process_events)),
    ])
    .unwrap();
    let (prepared, invocation, grant) = read_call();
    let execution = router.prepare(&invocation, &prepared, &grant).unwrap();
    assert_eq!(execution.executor_id, "workspace");
    assert_eq!(
        router
            .dispatch(ExecutorDispatch {
                invocation_id: &invocation,
                prepared: &prepared,
                grant: &grant,
                execution: &execution,
                receipt_id: "receipt-1",
            })
            .await,
        Err(ExecutorDispatchError::StartedWithoutReceipt)
    );
    router
        .reconcile_started_loss(ExecutorRecoveryRequest {
            invocation_id: &invocation,
            prepared_digest: prepared.input_digest(),
            executor_id: "workspace",
            executor_revision: "executor.v1",
            dispatch_attempt_id: "attempt-1",
        })
        .unwrap();
    assert_eq!(
        workspace_events.lock().unwrap().as_slice(),
        [
            "prepare:garive.workspace.read_text",
            "dispatch:workspace",
            "recover:workspace"
        ]
    );
    assert!(process_events.lock().unwrap().is_empty());
}

#[test]
fn rejects_duplicate_routes_and_a_lying_executor_identity() {
    let events = Arc::new(Mutex::new(Vec::new()));
    assert!(RoutedExecutorPort::new([
        route("one", [T1_READ_TEXT], Arc::clone(&events)),
        route("two", [T1_READ_TEXT], Arc::clone(&events)),
    ])
    .is_err());

    let mut router = RoutedExecutorPort::new([ExecutorRoute::new(
        "expected",
        [T1_READ_TEXT],
        Box::new(RecordingExecutor::new("different", events)),
    )
    .unwrap()])
    .unwrap();
    let (prepared, invocation, grant) = read_call();
    assert!(router.prepare(&invocation, &prepared, &grant).is_err());
}

fn route<const N: usize>(
    executor_id: &str,
    tools: [&str; N],
    events: Arc<Mutex<Vec<String>>>,
) -> ExecutorRoute {
    ExecutorRoute::new(
        executor_id,
        tools,
        Box::new(RecordingExecutor::new(executor_id, events)),
    )
    .unwrap()
}

fn read_call() -> (PreparedToolCall, ToolInvocationId, InvocationGrant) {
    let prepared = BuiltinT1Catalogue::new("policy.v1", ["process"])
        .unwrap()
        .prepare(&ToolIntent::new(
            "model-call",
            T1_READ_TEXT,
            r#"{"path":"README.md","max_bytes":4096}"#,
        ))
        .unwrap();
    let invocation = ToolInvocationId::new("invocation-1").unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("grant-1").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "policy.v1",
    )
    .unwrap();
    (prepared, invocation, grant)
}
