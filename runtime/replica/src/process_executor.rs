//! F0-bound executor for the T1 configured-process contract.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use garive_ledger::CanonicalPayload;
use garive_tools::{
    AccessMode, AccessNamespace, BuiltinT1Catalogue, EffectReceipt, ExecutionCapability,
    ExecutionFact, InvocationGrant, PreparedToolCall, ReceiptId, ReplayClass,
    TerminalClassification, ToolIntent, ToolInvocationId, T1_PROCESS_RUN,
};
use serde_json::{json, Value};

use crate::{
    t1_dispatch_attempt_id, ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort,
    ExecutorRecoveryRequest, PreparedExecution, ProcessLaneRegistry,
};

/// Stable executor identity used by matching F0 sandbox bindings.
pub const T1_PROCESS_EXECUTOR_ID: &str = "garive.builtin.process";

/// Exact workspace authority enforced for the launched process tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessWorkspaceMode {
    /// The process may read only within the granted working-directory subtree.
    Read,
    /// The process may read and write within the granted working-directory subtree.
    Write,
}

/// Exact bounded command delivered to a concrete isolation backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessExecutionRequest {
    /// Stable Runtime invocation identity used for backend job ownership.
    pub invocation_id: ToolInvocationId,
    /// Exact dispatch attempt selected before Started.
    pub dispatch_attempt_id: String,
    /// Exact configured lane identity.
    pub lane: String,
    /// Absolute executable capability resolved without PATH.
    pub executable: PathBuf,
    /// Exact non-empty argv vector, including the configured alias at index zero.
    pub argv: Vec<String>,
    /// Exact workspace-relative working-directory resource identity.
    pub working_directory: String,
    /// Exact workspace authority granted by Safety.
    pub workspace_mode: ProcessWorkspaceMode,
    /// Complete environment installed after clearing inherited values.
    pub environment: BTreeMap<String, String>,
    /// Aggregate stdout/stderr byte bound.
    pub max_output_bytes: u64,
    /// Wall-clock duration bound.
    pub timeout_ms: u64,
    /// Maximum process count admitted by the frozen sandbox profile.
    pub max_processes: u32,
    /// Maximum open-file count admitted by the frozen sandbox profile.
    pub max_open_files: u32,
}

/// Trustworthy terminal classification returned by an isolation backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessExit {
    /// Normal exit with the exact status code.
    Code(i32),
    /// Signal termination with the platform signal number.
    Signal(i32),
    /// Timeout after the complete process tree was terminated.
    Timeout,
}

/// Bounded terminal evidence returned by a concrete backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessExecutionResult {
    /// Terminal classification.
    pub exit: ProcessExit,
    /// Separately captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Separately captured stderr bytes.
    pub stderr: Vec<u8>,
    /// Whether output was cut at the aggregate bound.
    pub truncated: bool,
    /// Proof that no member of the launched process tree remains.
    pub process_tree_terminated: bool,
}

/// Failure before or after the concrete process boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessBackendError {
    /// Configured capability is temporarily unavailable before launch.
    Unavailable,
    /// Launch may have crossed without trustworthy terminal evidence.
    StateUnknown,
}

/// Native isolation backend; implementations own workspace and OS enforcement.
pub trait ProcessIsolationBackend: Send + Sync {
    /// Proves the exact command and controls are enforceable without launching it.
    fn preflight(&self, request: &ProcessExecutionRequest) -> Result<(), String>;

    /// Executes once, never replays, and returns bounded terminal evidence.
    fn execute(
        &self,
        request: ProcessExecutionRequest,
    ) -> Result<ProcessExecutionResult, ProcessBackendError>;

    /// Releases one stopped backend job after its receipt became durable.
    fn acknowledge_terminal(
        &self,
        invocation_id: &ToolInvocationId,
        dispatch_attempt_id: &str,
    ) -> Result<(), ProcessBackendError>;

    /// Idempotently terminates or proves absence of the exact backend job.
    fn terminate_or_prove_absent(
        &self,
        invocation_id: &ToolInvocationId,
        dispatch_attempt_id: &str,
    ) -> Result<(), ProcessBackendError>;
}

/// T1 process adapter binding the frozen catalogue, lanes and native backend.
pub struct BuiltinProcessExecutor {
    revision: String,
    catalogue: BuiltinT1Catalogue,
    lanes: ProcessLaneRegistry,
    backend: Arc<dyn ProcessIsolationBackend>,
}

impl BuiltinProcessExecutor {
    /// Constructs an executor exclusively from explicit Garive configuration.
    pub fn new(
        revision: impl Into<String>,
        catalogue: BuiltinT1Catalogue,
        lanes: ProcessLaneRegistry,
        backend: Arc<dyn ProcessIsolationBackend>,
    ) -> Result<Self, String> {
        let revision = revision.into();
        if revision.is_empty()
            || catalogue
                .definitions()
                .iter()
                .find(|definition| definition.name() == T1_PROCESS_RUN)
                .is_none()
        {
            return Err("invalid T1 process executor construction".into());
        }
        Ok(Self {
            revision,
            catalogue,
            lanes,
            backend,
        })
    }
}

impl ExecutorPort for BuiltinProcessExecutor {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        let request = operation(&self.catalogue, &self.lanes, invocation_id, prepared, grant)?;
        self.backend.preflight(&request)?;
        Ok(PreparedExecution {
            executor_id: T1_PROCESS_EXECUTOR_ID.into(),
            executor_revision: self.revision.clone(),
            dispatch_attempt_id: dispatch_id(invocation_id)?,
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        let request = operation(
            &self.catalogue,
            &self.lanes,
            command.invocation_id,
            command.prepared,
            command.grant,
        );
        let backend = Arc::clone(&self.backend);
        let expected_dispatch = dispatch_id(command.invocation_id).unwrap_or_default();
        Box::pin(async move {
            if command.execution.executor_id != T1_PROCESS_EXECUTOR_ID
                || command.execution.executor_revision != self.revision
                || command.execution.dispatch_attempt_id != expected_dispatch
            {
                return Err(ExecutorDispatchError::ReceiptInvalid);
            }
            let request = request.map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            let result = tokio::task::spawn_blocking(move || backend.execute(request))
                .await
                .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?
                .map_err(|error| match error {
                    ProcessBackendError::Unavailable => {
                        ExecutorDispatchError::StartedWithoutReceipt
                    }
                    ProcessBackendError::StateUnknown => {
                        ExecutorDispatchError::ExecutorStateUnknown
                    }
                })?;
            terminal(&command, result)
        })
    }

    fn reconcile_started_loss(
        &mut self,
        request: ExecutorRecoveryRequest<'_>,
    ) -> Result<(), ExecutorDispatchError> {
        if request.executor_id != T1_PROCESS_EXECUTOR_ID
            || request.executor_revision != self.revision
            || request.dispatch_attempt_id != dispatch_id(request.invocation_id).unwrap_or_default()
            || request.prepared_digest.is_empty()
        {
            return Err(ExecutorDispatchError::ReceiptInvalid);
        }
        self.backend
            .terminate_or_prove_absent(request.invocation_id, request.dispatch_attempt_id)
            .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)
    }

    fn acknowledge_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        if receipt.invocation_id != *invocation_id
            || receipt.executor_id != T1_PROCESS_EXECUTOR_ID
            || receipt.executor_revision != self.revision
        {
            return Err(ExecutorDispatchError::ReceiptInvalid);
        }
        self.backend
            .acknowledge_terminal(
                invocation_id,
                &dispatch_id(invocation_id).map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
            )
            .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)
    }
}

fn operation(
    catalogue: &BuiltinT1Catalogue,
    lanes: &ProcessLaneRegistry,
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
) -> Result<ProcessExecutionRequest, String> {
    if prepared.contract_version() != 3
        || prepared.tool_name() != T1_PROCESS_RUN
        || prepared.replay_class() != ReplayClass::NeverReplay
        || grant.invocation_id != *invocation_id
        || grant.prepared_digest != prepared.input_digest()
        || grant.tool_name != prepared.tool_name()
        || grant.tool_revision != prepared.tool_revision()
    {
        return Err("invalid T1 process binding".into());
    }
    let reconstructed = catalogue
        .prepare(&ToolIntent::new(
            prepared.model_call_id(),
            prepared.tool_name(),
            prepared.normalized_arguments(),
        ))
        .map_err(|_| "T1 process definition mismatch")?;
    if reconstructed != *prepared {
        return Err("T1 process definition mismatch".into());
    }
    let arguments: Value = serde_json::from_str(prepared.normalized_arguments())
        .map_err(|_| "invalid T1 process arguments")?;
    let lane_name = text(&arguments, "lane")?;
    let working_directory = text(&arguments, "working_directory")?;
    let workspace_mode = match text(&arguments, "workspace_mode")? {
        "read" => ProcessWorkspaceMode::Read,
        "write" => ProcessWorkspaceMode::Write,
        _ => return Err("invalid process workspace mode".into()),
    };
    validate_accesses(prepared, lane_name, working_directory, workspace_mode)?;
    validate_requirements(prepared, grant)?;
    let argv = arguments
        .get("argv")
        .and_then(Value::as_array)
        .ok_or("invalid process argv")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or("invalid process argv")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let alias = argv.first().ok_or("empty process argv")?;
    let lane = lanes.lane(lane_name).ok_or("process lane unavailable")?;
    let executable = lane
        .executable(alias)
        .ok_or("process executable unavailable")?;
    let sandbox = prepared
        .sandbox_requirements()
        .ok_or("missing process sandbox requirements")?;
    Ok(ProcessExecutionRequest {
        invocation_id: invocation_id.clone(),
        dispatch_attempt_id: dispatch_id(invocation_id)?,
        lane: lane_name.into(),
        executable: executable.path().to_path_buf(),
        argv,
        working_directory: working_directory.into(),
        workspace_mode,
        environment: lane.environment().clone(),
        max_output_bytes: number(&arguments, "max_output_bytes")?
            .min(grant.granted_requirements.max_output_bytes()),
        timeout_ms: number(&arguments, "timeout_ms")?
            .min(grant.granted_requirements.max_duration_ms()),
        max_processes: sandbox.max_processes().ok_or("missing process bound")?,
        max_open_files: sandbox.max_open_files(),
    })
}

fn validate_accesses(
    prepared: &PreparedToolCall,
    lane: &str,
    working_directory: &str,
    workspace_mode: ProcessWorkspaceMode,
) -> Result<(), String> {
    let accesses = prepared
        .invocation_accesses()
        .ok_or("missing process accesses")?;
    let process = accesses.values().iter().find(|access| {
        access.namespace() == AccessNamespace::Process && access.mode() == AccessMode::Exclusive
    });
    let expected_mode = match workspace_mode {
        ProcessWorkspaceMode::Read => AccessMode::Read,
        ProcessWorkspaceMode::Write => AccessMode::Write,
    };
    let filesystem = accesses.values().iter().find(|access| {
        access.namespace() == AccessNamespace::Filesystem && access.mode() == expected_mode
    });
    if accesses.values().len() != 2
        || process.map(|value| value.resource_key()) != Some(lane)
        || filesystem.map(|value| value.resource_key()) != Some(working_directory)
    {
        return Err("T1 process access mismatch".into());
    }
    Ok(())
}

fn validate_requirements(
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
) -> Result<(), String> {
    let capabilities = [
        ExecutionCapability::FilesystemRead,
        ExecutionCapability::FilesystemWrite,
        ExecutionCapability::Process,
    ];
    if !prepared.requirements().capabilities().eq(capabilities)
        || !grant.granted_requirements.capabilities().eq(capabilities)
        || grant.granted_requirements.max_duration_ms() > prepared.requirements().max_duration_ms()
        || grant.granted_requirements.max_output_bytes()
            > prepared.requirements().max_output_bytes()
    {
        return Err("T1 process requirements mismatch".into());
    }
    Ok(())
}

fn terminal(
    command: &ExecutorDispatch<'_>,
    result: ProcessExecutionResult,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    if !result.process_tree_terminated
        || result.stdout.len().saturating_add(result.stderr.len())
            > usize::try_from(command.grant.granted_requirements.max_output_bytes())
                .unwrap_or(usize::MAX)
    {
        return Err(ExecutorDispatchError::ExecutorStateUnknown);
    }
    let stdout =
        String::from_utf8(result.stdout).map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
    let stderr =
        String::from_utf8(result.stderr).map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
    let (exit_kind, exit_code, failure) = match result.exit {
        ProcessExit::Code(0) => ("code", Some(0), None),
        ProcessExit::Code(code) => ("code", Some(code), Some("process_exit_nonzero")),
        ProcessExit::Signal(signal) => ("signal", Some(signal), Some("process_signal")),
        ProcessExit::Timeout => ("timeout", None, Some("process_timeout")),
    };
    let content = json!({"exit_kind":exit_kind,"exit_code":exit_code,"stdout":stdout,
        "stderr":stderr,"truncated":result.truncated});
    match failure {
        None => completed(command, content, result.truncated),
        Some(code) => failed(command, code, content),
    }
}

fn completed(
    command: &ExecutorDispatch<'_>,
    content: Value,
    truncated: bool,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let digest = canonical_digest(&content)?;
    Ok(ExecutionFact::Completed {
        receipt: Some(receipt(command, TerminalClassification::Completed, digest)?),
        content,
        truncated,
    })
}

fn failed(
    command: &ExecutorDispatch<'_>,
    code: &str,
    partial: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let evidence = json!({"code":code,"details":null,"partial":partial});
    let digest = canonical_digest(&evidence)?;
    Ok(ExecutionFact::Failed {
        receipt: Some(receipt(command, TerminalClassification::Failed, digest)?),
        code: code.into(),
        details: None,
        partial: evidence.get("partial").cloned(),
    })
}

fn receipt(
    command: &ExecutorDispatch<'_>,
    terminal_classification: TerminalClassification,
    result_digest: String,
) -> Result<EffectReceipt, ExecutorDispatchError> {
    Ok(EffectReceipt {
        receipt_id: ReceiptId::new(command.receipt_id)
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
        invocation_id: command.invocation_id.clone(),
        prepared_digest: command.prepared.input_digest().into(),
        grant_id: command.grant.grant_id.clone(),
        executor_id: command.execution.executor_id.clone(),
        executor_revision: command.execution.executor_revision.clone(),
        terminal_classification,
        result_digest,
    })
}

fn canonical_digest(value: &Value) -> Result<String, ExecutorDispatchError> {
    CanonicalPayload::from_value(value)
        .map(|payload| payload.sha256().to_owned())
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)
}

fn text<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("invalid process {name}"))
}

fn number(arguments: &Value, name: &str) -> Result<u64, String> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("invalid process {name}"))
}

fn dispatch_id(invocation: &ToolInvocationId) -> Result<String, String> {
    t1_dispatch_attempt_id(T1_PROCESS_EXECUTOR_ID, invocation)
        .ok_or_else(|| "invalid T1 executor identity".to_owned())
}
