//! F0-bound executor adapter for the T1 journaled patch contract.

use std::{collections::BTreeMap, path::Path};

use garive_ledger::CanonicalPayload;
use garive_tools::{
    AccessMode, AccessNamespace, BuiltinT1Catalogue, EffectReceipt, ExecutionCapability,
    ExecutionFact, InvocationGrant, PreparedToolCall, ReceiptId, ReplayClass,
    TerminalClassification, ToolIntent, ToolInvocationId, T1_APPLY_PATCH,
};
use rustix::{
    fd::OwnedFd,
    fs::{open, Mode, OFlags},
    io::dup,
};
use serde_json::{json, Value};

use crate::{
    builtin_patch_journal::{acknowledge_patch, execute_patch, PatchFailure},
    t1_dispatch_attempt_id, ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort,
    PreparedExecution,
};

/// Stable executor identity used by F0 patch bindings.
pub const T1_PATCH_EXECUTOR_ID: &str = "garive.builtin.patch";

/// Unix descriptor-confined T1 journaled-patch executor.
pub struct BuiltinPatchExecutor {
    root: OwnedFd,
    recovery: OwnedFd,
    revision: String,
    catalogue: BuiltinT1Catalogue,
}

impl BuiltinPatchExecutor {
    /// Opens explicit Workspace and Runtime-private recovery capabilities.
    pub fn new(
        workspace_root: impl AsRef<Path>,
        recovery_root: impl AsRef<Path>,
        revision: impl Into<String>,
        catalogue: BuiltinT1Catalogue,
    ) -> Result<Self, std::io::Error> {
        let revision = revision.into();
        if revision.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "empty T1 patch executor revision",
            ));
        }
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        Ok(Self {
            root: open(workspace_root.as_ref(), flags, Mode::empty())?,
            recovery: open(recovery_root.as_ref(), flags, Mode::empty())?,
            revision,
            catalogue,
        })
    }
}

impl ExecutorPort for BuiltinPatchExecutor {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        patch_operation(&self.catalogue, invocation_id, prepared, grant)?;
        Ok(PreparedExecution {
            executor_id: T1_PATCH_EXECUTOR_ID.into(),
            executor_revision: self.revision.clone(),
            dispatch_attempt_id: t1_dispatch_attempt_id(T1_PATCH_EXECUTOR_ID, invocation_id)
                .ok_or_else(|| "invalid T1 executor identity".to_owned())?,
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        let operation = patch_operation(
            &self.catalogue,
            command.invocation_id,
            command.prepared,
            command.grant,
        );
        let root = dup(&self.root);
        let recovery = dup(&self.recovery);
        let expected_dispatch =
            t1_dispatch_attempt_id(T1_PATCH_EXECUTOR_ID, command.invocation_id).unwrap_or_default();
        Box::pin(async move {
            if command.execution.executor_id != T1_PATCH_EXECUTOR_ID
                || command.execution.executor_revision != self.revision
                || command.execution.dispatch_attempt_id != expected_dispatch
            {
                return Err(ExecutorDispatchError::ReceiptInvalid);
            }
            let operation = operation.map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            let root = root.map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            let recovery = recovery.map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            let invocation = command.invocation_id.as_str().to_owned();
            let prepared_digest = command.prepared.input_digest().to_owned();
            let result = tokio::task::spawn_blocking(move || {
                execute_patch(
                    root,
                    recovery,
                    &invocation,
                    &prepared_digest,
                    &operation.patch,
                    &operation.expected,
                    operation.result_bound,
                )
            })
            .await
            .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            match result {
                Ok(content) => completed(&command, content),
                Err(PatchFailure::Uncertain) => Err(ExecutorDispatchError::ExecutorStateUnknown),
                Err(error) => failed(&command, failure_code(error)),
            }
        })
    }

    fn acknowledge_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        if receipt.invocation_id != *invocation_id
            || receipt.executor_id != T1_PATCH_EXECUTOR_ID
            || receipt.executor_revision != self.revision
        {
            return Err(ExecutorDispatchError::ReceiptInvalid);
        }
        acknowledge_patch(&self.recovery, invocation_id.as_str(), receipt)
            .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)
    }
}

struct PatchOperation {
    patch: String,
    expected: BTreeMap<String, String>,
    result_bound: u64,
}

fn patch_operation(
    catalogue: &BuiltinT1Catalogue,
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
) -> Result<PatchOperation, String> {
    if prepared.contract_version() != 3
        || prepared.tool_name() != T1_APPLY_PATCH
        || prepared.replay_class() != ReplayClass::ReceiptRecoverable
        || grant.invocation_id != *invocation_id
        || grant.prepared_digest != prepared.input_digest()
        || grant.tool_name != prepared.tool_name()
        || grant.tool_revision != prepared.tool_revision()
        || !requirements_match(prepared, grant)
    {
        return Err("invalid T1 patch binding".into());
    }
    let reconstructed = catalogue
        .prepare(&ToolIntent::new(
            prepared.model_call_id(),
            prepared.tool_name(),
            prepared.normalized_arguments(),
        ))
        .map_err(|_| "T1 patch definition mismatch")?;
    if reconstructed != *prepared {
        return Err("T1 patch definition mismatch".into());
    }
    let accesses = prepared
        .invocation_accesses()
        .ok_or("missing T1 patch access")?;
    if accesses.values().iter().any(|access| {
        access.namespace() != AccessNamespace::Filesystem || access.mode() != AccessMode::Write
    }) {
        return Err("invalid T1 patch access".into());
    }
    let arguments: Value = serde_json::from_str(prepared.normalized_arguments())
        .map_err(|_| "invalid T1 patch arguments")?;
    let patch = arguments
        .get("patch")
        .and_then(Value::as_str)
        .ok_or("invalid T1 patch")?
        .to_owned();
    let mut expected = BTreeMap::new();
    for file in arguments
        .get("expected_files")
        .and_then(Value::as_array)
        .ok_or("invalid expected files")?
    {
        let path = file
            .get("path")
            .and_then(Value::as_str)
            .ok_or("invalid expected path")?;
        let digest = file
            .get("before_digest")
            .and_then(Value::as_str)
            .ok_or("invalid expected digest")?;
        expected.insert(path.to_owned(), digest.to_owned());
    }
    if expected
        .keys()
        .map(String::as_str)
        .ne(accesses.values().iter().map(|access| access.resource_key()))
    {
        return Err("T1 patch access mismatch".into());
    }
    Ok(PatchOperation {
        patch,
        expected,
        result_bound: prepared
            .max_result_bytes()
            .ok_or("missing result bound")?
            .min(grant.granted_requirements.max_output_bytes()),
    })
}

fn requirements_match(prepared: &PreparedToolCall, grant: &InvocationGrant) -> bool {
    let capabilities = [
        ExecutionCapability::FilesystemRead,
        ExecutionCapability::FilesystemWrite,
    ];
    prepared.requirements().capabilities().eq(capabilities)
        && grant.granted_requirements.capabilities().eq(capabilities)
        && grant.granted_requirements.max_duration_ms() <= prepared.requirements().max_duration_ms()
        && grant.granted_requirements.max_output_bytes()
            <= prepared.requirements().max_output_bytes()
}

const fn failure_code(error: PatchFailure) -> &'static str {
    match error {
        PatchFailure::NotFound => "path_not_found",
        PatchFailure::AccessDenied => "access_denied",
        PatchFailure::NonUtf8 => "non_utf8_content",
        PatchFailure::BoundExceeded => "result_bound_exceeded",
        PatchFailure::ContentChanged => "content_changed",
        PatchFailure::Conflict => "patch_conflict",
        PatchFailure::Uncertain => "executor_state_unknown",
    }
}

fn completed(
    command: &ExecutorDispatch<'_>,
    content: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let result_digest = CanonicalPayload::from_value(&content)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Completed {
        receipt: Some(receipt(
            command,
            TerminalClassification::Completed,
            result_digest,
        )?),
        content,
        truncated: false,
    })
}

fn failed(
    command: &ExecutorDispatch<'_>,
    code: &str,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let evidence = json!({"code":code,"details":null,"partial":null});
    let result_digest = CanonicalPayload::from_value(&evidence)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Failed {
        receipt: Some(receipt(
            command,
            TerminalClassification::Failed,
            result_digest,
        )?),
        code: code.into(),
        details: None,
        partial: None,
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
