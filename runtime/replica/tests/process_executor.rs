use std::sync::{Arc, Mutex};

use garive_runtime::{
    BuiltinProcessExecutor, ExecutorDispatch, ExecutorDispatchError, ExecutorPort,
    ExecutorRecoveryRequest, ProcessBackendError, ProcessExecutable, ProcessExecutionRequest,
    ProcessExecutionResult, ProcessExit, ProcessIsolationBackend, ProcessLane, ProcessLaneRegistry,
    ProcessWorkspaceMode, T1_PROCESS_EXECUTOR_ID,
};
use garive_tools::{
    BuiltinT1Catalogue, ExecutionFact, GrantId, InvocationGrant, ToolIntent, ToolInvocationId,
    T1_PROCESS_RUN,
};
use sha2::{Digest, Sha256};

struct RecordingBackend {
    preflighted: Mutex<Vec<ProcessExecutionRequest>>,
    executed: Mutex<Vec<ProcessExecutionRequest>>,
    result: Mutex<Result<ProcessExecutionResult, ProcessBackendError>>,
    reconciled: Mutex<Vec<(String, String)>>,
}

impl RecordingBackend {
    fn new(result: Result<ProcessExecutionResult, ProcessBackendError>) -> Self {
        Self {
            preflighted: Mutex::new(Vec::new()),
            executed: Mutex::new(Vec::new()),
            result: Mutex::new(result),
            reconciled: Mutex::new(Vec::new()),
        }
    }
}

impl ProcessIsolationBackend for RecordingBackend {
    fn preflight(&self, request: &ProcessExecutionRequest) -> Result<(), String> {
        self.preflighted.lock().unwrap().push(request.clone());
        Ok(())
    }

    fn execute(
        &self,
        request: ProcessExecutionRequest,
    ) -> Result<ProcessExecutionResult, ProcessBackendError> {
        self.executed.lock().unwrap().push(request);
        self.result.lock().unwrap().clone()
    }

    fn terminate_or_prove_absent(
        &self,
        invocation_id: &ToolInvocationId,
        dispatch_attempt_id: &str,
    ) -> Result<(), ProcessBackendError> {
        self.reconciled
            .lock()
            .unwrap()
            .push((invocation_id.as_str().into(), dispatch_attempt_id.into()));
        Ok(())
    }
}

#[tokio::test]
async fn process_resolves_exact_capability_and_returns_bound_receipt() {
    let backend = Arc::new(RecordingBackend::new(Ok(result(ProcessExit::Code(0)))));
    let (fact, _) = dispatch(Arc::clone(&backend), "cargo", None).await.unwrap();

    let ExecutionFact::Completed {
        receipt: Some(receipt),
        content,
        truncated,
    } = fact
    else {
        panic!("expected completed process")
    };
    assert_eq!(receipt.executor_id, T1_PROCESS_EXECUTOR_ID);
    assert_eq!(content["exit_kind"], "code");
    assert_eq!(content["exit_code"], 0);
    assert_eq!(content["stdout"], "stdout");
    assert!(!truncated);

    let preflighted = backend.preflighted.lock().unwrap();
    assert_eq!(preflighted.len(), 1);
    let request = &preflighted[0];
    assert_eq!(request.invocation_id.as_str(), "process-invocation");
    assert!(request.dispatch_attempt_id.starts_with("process-dispatch-"));
    assert_eq!(request.lane, "rust-toolchain");
    assert_eq!(
        request.executable,
        std::path::Path::new("/configured/cargo")
    );
    assert_eq!(request.argv, ["cargo", "test", "-p", "garive-tools"]);
    assert_eq!(request.working_directory, ".");
    assert_eq!(request.workspace_mode, ProcessWorkspaceMode::Write);
    assert_eq!(request.environment.len(), 1);
    assert_eq!(
        request.environment.get("LANG").map(String::as_str),
        Some("C")
    );
    assert_eq!(request.max_output_bytes, 4_096);
    assert_eq!(request.timeout_ms, 30_000);
    assert_eq!(request.max_processes, 16);
    assert_eq!(request.max_open_files, 64);
    assert_eq!(
        backend.executed.lock().unwrap().as_slice(),
        preflighted.as_slice()
    );
}

#[tokio::test]
async fn unconfigured_argv_zero_fails_before_preflight_or_started_dispatch() {
    let backend = Arc::new(RecordingBackend::new(Ok(result(ProcessExit::Code(0)))));
    let error = dispatch(Arc::clone(&backend), "sh", None)
        .await
        .err()
        .expect("unconfigured alias must fail");

    assert_eq!(error, ExecutorDispatchError::ReceiptInvalid);
    assert!(backend.preflighted.lock().unwrap().is_empty());
    assert!(backend.executed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn nonzero_and_timeout_are_receipted_failures_with_bounded_partial_output() {
    for (exit, expected_code, expected_kind) in [
        (ProcessExit::Code(7), "process_exit_nonzero", "code"),
        (ProcessExit::Timeout, "process_timeout", "timeout"),
    ] {
        let backend = Arc::new(RecordingBackend::new(Ok(result(exit))));
        let (fact, _) = dispatch(backend, "cargo", None).await.unwrap();
        let ExecutionFact::Failed {
            receipt: Some(receipt),
            code,
            partial: Some(partial),
            ..
        } = fact
        else {
            panic!("expected receipted process failure")
        };
        assert_eq!(code, expected_code);
        assert_eq!(partial["exit_kind"], expected_kind);
        assert_eq!(partial["stdout"], "stdout");
        assert_eq!(
            receipt.terminal_classification,
            garive_tools::TerminalClassification::Failed
        );
    }
}

#[tokio::test]
async fn missing_process_tree_termination_proof_is_uncertain() {
    let mut outcome = result(ProcessExit::Timeout);
    outcome.process_tree_terminated = false;
    let backend = Arc::new(RecordingBackend::new(Ok(outcome)));
    let error = dispatch(backend, "cargo", None)
        .await
        .err()
        .expect("missing tree proof must be uncertain");

    assert_eq!(error, ExecutorDispatchError::ExecutorStateUnknown);
}

#[tokio::test]
async fn changed_executor_attempt_never_reaches_backend() {
    let backend = Arc::new(RecordingBackend::new(Ok(result(ProcessExit::Code(0)))));
    let error = dispatch(Arc::clone(&backend), "cargo", Some("changed-attempt"))
        .await
        .err()
        .expect("changed attempt must fail");

    assert_eq!(error, ExecutorDispatchError::ReceiptInvalid);
    assert!(backend.executed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn lost_started_process_delegates_exact_idempotent_cleanup() {
    let backend = Arc::new(RecordingBackend::new(Ok(result(ProcessExit::Code(0)))));
    let (fact, mut executor) = dispatch(Arc::clone(&backend), "cargo", None).await.unwrap();
    let invocation = ToolInvocationId::new("process-invocation").unwrap();
    let digest = format!("{:x}", Sha256::digest(invocation.as_str().as_bytes()));
    let attempt = format!("process-dispatch-{}", &digest[..24]);
    let prepared_digest = match fact {
        ExecutionFact::Completed {
            receipt: Some(receipt),
            ..
        } => receipt.prepared_digest,
        _ => panic!("expected process receipt"),
    };

    executor
        .reconcile_started_loss(ExecutorRecoveryRequest {
            invocation_id: &invocation,
            prepared_digest: &prepared_digest,
            executor_id: T1_PROCESS_EXECUTOR_ID,
            executor_revision: "process-v1",
            dispatch_attempt_id: &attempt,
        })
        .unwrap();
    assert_eq!(
        backend.reconciled.lock().unwrap().as_slice(),
        [("process-invocation".into(), attempt)]
    );
}

async fn dispatch(
    backend: Arc<RecordingBackend>,
    alias: &str,
    changed_attempt: Option<&str>,
) -> Result<(ExecutionFact, BuiltinProcessExecutor), ExecutorDispatchError> {
    let catalogue = BuiltinT1Catalogue::new("snapshot-1", ["rust-toolchain"]).unwrap();
    let arguments = format!(
        r#"{{"lane":"rust-toolchain","argv":["{alias}","test","-p","garive-tools"],"working_directory":".","workspace_mode":"write","max_output_bytes":4096,"timeout_ms":30000}}"#
    );
    let prepared = catalogue
        .prepare(&ToolIntent::new("model-call", T1_PROCESS_RUN, arguments))
        .unwrap();
    let invocation = ToolInvocationId::new("process-invocation").unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("process-grant").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "a".repeat(64),
        "snapshot-1",
    )
    .unwrap();
    let lanes = ProcessLaneRegistry::new([ProcessLane::new(
        "rust-toolchain",
        [ProcessExecutable::new("cargo", "/configured/cargo").unwrap()],
        [("LANG".into(), "C".into())],
    )
    .unwrap()])
    .unwrap();
    let backend_port: Arc<dyn ProcessIsolationBackend> = backend;
    let mut executor =
        BuiltinProcessExecutor::new("process-v1", catalogue, lanes, backend_port).unwrap();
    let mut execution = executor
        .prepare(&invocation, &prepared, &grant)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
    if let Some(attempt) = changed_attempt {
        execution.dispatch_attempt_id = attempt.into();
    }
    let fact = executor
        .dispatch(ExecutorDispatch {
            invocation_id: &invocation,
            prepared: &prepared,
            grant: &grant,
            execution: &execution,
            receipt_id: "process-receipt",
        })
        .await?;
    Ok((fact, executor))
}

fn result(exit: ProcessExit) -> ProcessExecutionResult {
    ProcessExecutionResult {
        exit,
        stdout: b"stdout".to_vec(),
        stderr: b"stderr".to_vec(),
        truncated: false,
        process_tree_terminated: true,
    }
}
