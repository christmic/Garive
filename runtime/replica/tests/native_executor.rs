use garive_runtime::{
    ExecutorDispatch, ExecutorDispatchError, ExecutorPort, NativeActionCommandV1,
    NativeActionFuture, NativeActionReceiptV1, NativeAdapterBindingV1, NativeAdapterPort,
    NativeCapabilityExecutor, NativeObservationBounds, NativeObservationFuture,
    NativeObservationV1, NativeProtocolError, NativeSnapshotId, NativeTarget,
    T2_NATIVE_EXECUTOR_ID,
};
use garive_tools::{
    BrowserPageScope, BuiltinT2BrowserCatalogue, BuiltinT2ComputerCatalogue, ComputerTargetScope,
    ExecutionFact, GrantId, InvocationGrant, PreparedToolCall, ToolIntent, ToolInvocationId,
    T2_BROWSER_OBSERVE, T2_COMPUTER_ACT,
};
use serde_json::json;

struct FakeAdapter {
    uncertain: bool,
}

impl NativeAdapterPort for FakeAdapter {
    fn observe<'a>(
        &'a mut self,
        target: &'a NativeTarget,
        _previous: Option<&'a NativeSnapshotId>,
        bounds: NativeObservationBounds,
    ) -> NativeObservationFuture<'a> {
        Box::pin(async move {
            Ok(NativeObservationV1 {
                target: target.clone(),
                snapshot_id: NativeSnapshotId::new("snapshot-new")?,
                target_revision: "revision-new".into(),
                nodes: vec![],
                focused_node: None,
                screenshot_reference: None,
                redacted_field_count: 0,
                bounds,
            })
        })
    }

    fn preflight_action(
        &mut self,
        _command: &NativeActionCommandV1,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
        Ok(binding())
    }

    fn dispatch_action<'a>(
        &'a mut self,
        command: &'a NativeActionCommandV1,
        binding: &'a NativeAdapterBindingV1,
    ) -> NativeActionFuture<'a> {
        Box::pin(async move {
            if self.uncertain {
                return Err(NativeProtocolError::ActionUncertain);
            }
            Ok(NativeActionReceiptV1 {
                action_id: command.action_id.clone(),
                prior_snapshot_id: command.expected_snapshot_id.clone(),
                binding: binding.clone(),
                terminal_classification: "completed".into(),
                native_evidence_digest: "b".repeat(64),
                resulting_snapshot_id: Some(NativeSnapshotId::new("snapshot-after")?),
            })
        })
    }
}

fn binding() -> NativeAdapterBindingV1 {
    NativeAdapterBindingV1 {
        adapter_id: "fake-native".into(),
        adapter_revision: "1".into(),
        preflight_evidence_digest: "a".repeat(64),
    }
}

fn executor(uncertain: bool) -> NativeCapabilityExecutor<FakeAdapter> {
    NativeCapabilityExecutor::new(
        "native-test-1",
        BuiltinT2BrowserCatalogue::new(
            "policy-1",
            [BrowserPageScope::new("browser-1", "page-1").expect("page")],
            ["https://example.test:443"],
        )
        .expect("browser catalogue"),
        BuiltinT2ComputerCatalogue::new(
            "policy-1",
            [ComputerTargetScope::new("desktop-1", "app-1", "window-1").expect("target")],
        )
        .expect("computer catalogue"),
        FakeAdapter { uncertain },
    )
    .expect("executor")
}

fn grant(invocation: &ToolInvocationId, prepared: &PreparedToolCall) -> InvocationGrant {
    InvocationGrant::new(
        GrantId::new("grant-1").expect("grant id"),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "constraints-1",
        "policy-1",
    )
    .expect("grant")
}

#[tokio::test]
async fn observe_returns_a_bounded_governed_completion() {
    let mut executor = executor(false);
    let prepared = BuiltinT2BrowserCatalogue::new(
        "policy-1",
        [BrowserPageScope::new("browser-1", "page-1").expect("page")],
        ["https://example.test:443"],
    )
    .expect("catalogue")
    .prepare(&ToolIntent::new(
        "call-1",
        T2_BROWSER_OBSERVE,
        r#"{"session_id":"browser-1","page_id":"page-1","max_nodes":10,"max_text_bytes":1000}"#,
    ))
    .expect("prepared");
    let invocation = ToolInvocationId::new("invocation-1").expect("invocation");
    let grant = grant(&invocation, &prepared);
    let execution = executor
        .prepare(&invocation, &prepared, &grant)
        .expect("preflight");
    assert_eq!(execution.executor_id, T2_NATIVE_EXECUTOR_ID);
    let fact = executor
        .dispatch(ExecutorDispatch {
            invocation_id: &invocation,
            prepared: &prepared,
            grant: &grant,
            execution: &execution,
            receipt_id: "receipt-1",
        })
        .await
        .expect("dispatch");
    let ExecutionFact::Completed {
        receipt: Some(receipt),
        content,
        truncated: false,
    } = fact
    else {
        panic!("expected governed completion")
    };
    assert_eq!(receipt.executor_id, T2_NATIVE_EXECUTOR_ID);
    assert_eq!(content["snapshot_id"], "snapshot-new");
    assert_eq!(content["target"]["domain"], "browser");
}

#[tokio::test]
async fn uncertain_native_action_never_becomes_a_terminal_failure() {
    let mut executor = executor(true);
    let prepared = BuiltinT2ComputerCatalogue::new(
        "policy-1",
        [ComputerTargetScope::new("desktop-1", "app-1", "window-1").expect("target")],
    )
    .expect("catalogue")
    .prepare(&ToolIntent::new(
        "call-2",
        T2_COMPUTER_ACT,
        json!({"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"revision-1","action":"press","node_ref":"node-1"}).to_string(),
    ))
    .expect("prepared");
    let invocation = ToolInvocationId::new("invocation-2").expect("invocation");
    let grant = grant(&invocation, &prepared);
    let execution = executor
        .prepare(&invocation, &prepared, &grant)
        .expect("preflight");
    assert_eq!(
        executor
            .dispatch(ExecutorDispatch {
                invocation_id: &invocation,
                prepared: &prepared,
                grant: &grant,
                execution: &execution,
                receipt_id: "receipt-2",
            })
            .await,
        Err(ExecutorDispatchError::StartedWithoutReceipt)
    );
}
