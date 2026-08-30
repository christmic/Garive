//! Descriptor-confined read-only executor for exact C5b filesystem accesses.

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
};

use garive_ledger::CanonicalPayload;
use garive_tools::{
    AccessMode, AccessNamespace, EffectReceipt, ExecutionFact, ReceiptId, ReplayClass,
    TerminalClassification, ToolInvocationId,
};
use rustix::{
    fd::OwnedFd,
    fs::{open, openat, Mode, OFlags},
    io::dup,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    AuthorizedBatchInvocation, CancellationEvidence, ConcurrentExecutorDispatch,
    ConcurrentExecutorPort, EffectCancellation, ExecutorDispatchError, PreparedExecution,
};

/// Read-only executor rooted at an explicitly supplied directory descriptor.
pub struct ConfinedFileReadExecutor {
    root: OwnedFd,
    root_path: PathBuf,
    revision: String,
}

impl ConfinedFileReadExecutor {
    /// Opens and freezes the workspace root without consulting environment state.
    pub fn new(
        workspace_root: impl AsRef<Path>,
        revision: impl Into<String>,
    ) -> Result<Self, std::io::Error> {
        let revision = revision.into();
        if revision.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "empty executor revision",
            ));
        }
        let root_path = workspace_root.as_ref().to_path_buf();
        let root = open(
            &root_path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Self {
            root,
            root_path,
            revision,
        })
    }
}

impl ConcurrentExecutorPort for ConfinedFileReadExecutor {
    fn prepare(&self, invocation: &AuthorizedBatchInvocation) -> Result<PreparedExecution, String> {
        invocation_path(invocation)?;
        Ok(PreparedExecution {
            executor_id: "garive.confined-file-read".into(),
            executor_revision: self.revision.clone(),
            dispatch_attempt_id: format!(
                "dispatch-{:x}",
                Sha256::digest(invocation.invocation_id.as_str().as_bytes())
            ),
        })
    }

    fn dispatch<'a>(
        &'a self,
        command: ConcurrentExecutorDispatch,
        cancellation: EffectCancellation,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<ExecutionFact, ExecutorDispatchError>>
                + Send
                + 'a,
        >,
    > {
        let root = dup(&self.root);
        let root_path = self.root_path.clone();
        Box::pin(async move {
            let root = root.map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            let path = command_path(&command).map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            if cancellation.is_cancelled() {
                return failure(&command, "cancelled");
            }
            let bound = command
                .prepared
                .max_result_bytes()
                .ok_or(ExecutorDispatchError::ReceiptInvalid)?;
            let result =
                tokio::task::spawn_blocking(move || confined_read(root, &root_path, &path, bound))
                    .await
                    .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            if cancellation.is_cancelled() {
                return failure(&command, "cancelled");
            }
            match result {
                Ok(content) => completed(&command, content),
                Err(_) => failure(&command, "access_denied"),
            }
        })
    }

    fn cancellation_evidence(&self, _: &ToolInvocationId) -> CancellationEvidence {
        CancellationEvidence::Unknown
    }
}

fn invocation_path(invocation: &AuthorizedBatchInvocation) -> Result<String, String> {
    if invocation.prepared.contract_version() != 2
        || invocation.prepared.replay_class() != ReplayClass::ReadOnly
        || invocation.grant.invocation_id != invocation.invocation_id
        || invocation.grant.prepared_digest != invocation.prepared.input_digest()
    {
        return Err("invalid binding".into());
    }
    let accesses = invocation
        .prepared
        .invocation_accesses()
        .ok_or_else(|| "missing accesses".to_owned())?
        .values();
    let [access] = accesses else {
        return Err("one exact access required".into());
    };
    if access.namespace() != AccessNamespace::Filesystem || access.mode() != AccessMode::Read {
        return Err("read-only filesystem access required".into());
    }
    let arguments: Value = serde_json::from_str(invocation.prepared.normalized_arguments())
        .map_err(|_| "invalid arguments")?;
    if arguments.get("path").and_then(Value::as_str) != Some(access.resource_key()) {
        return Err("path/access mismatch".into());
    }
    Ok(access.resource_key().into())
}

fn command_path(command: &ConcurrentExecutorDispatch) -> Result<String, String> {
    invocation_path(&AuthorizedBatchInvocation {
        invocation_id: command.invocation_id.clone(),
        prepared: command.prepared.clone(),
        grant: command.grant.clone(),
        receipt_id: command.receipt_id.clone(),
    })
}

fn confined_read(
    root: OwnedFd,
    root_path: &Path,
    path: &str,
    bound: u64,
) -> std::io::Result<Value> {
    let mut directory = root;
    let mut visible_directory = root_path.to_path_buf();
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        if !has_exact_component(&visible_directory, component)? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "non-exact component",
            ));
        }
        let last = components.peek().is_none();
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | if last {
                OFlags::empty()
            } else {
                OFlags::DIRECTORY
            };
        let next = openat(&directory, component, flags, Mode::empty())?;
        if last {
            let mut bytes = Vec::new();
            File::from(next)
                .take(bound.saturating_add(1))
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > bound {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "result bound",
                ));
            }
            let text = String::from_utf8(bytes)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "non UTF-8"))?;
            return Ok(json!({"text":text}));
        }
        directory = next;
        visible_directory.push(component);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "empty path",
    ))
}

fn has_exact_component(directory: &Path, component: &str) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(directory)? {
        if entry?.file_name().as_encoded_bytes() == component.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn completed(
    command: &ConcurrentExecutorDispatch,
    content: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let digest = CanonicalPayload::from_value(&content)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Completed {
        receipt: Some(receipt(command, TerminalClassification::Completed, digest)?),
        content,
        truncated: false,
    })
}

fn failure(
    command: &ConcurrentExecutorDispatch,
    code: &str,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let evidence = json!({"code":code,"details":null,"partial":null});
    let digest = CanonicalPayload::from_value(&evidence)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Failed {
        receipt: Some(receipt(command, TerminalClassification::Failed, digest)?),
        code: code.into(),
        details: None,
        partial: None,
    })
}

fn receipt(
    command: &ConcurrentExecutorDispatch,
    classification: TerminalClassification,
    result_digest: String,
) -> Result<EffectReceipt, ExecutorDispatchError> {
    Ok(EffectReceipt {
        receipt_id: ReceiptId::new(command.receipt_id.clone())
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
        invocation_id: command.invocation_id.clone(),
        prepared_digest: command.prepared.input_digest().into(),
        grant_id: command.grant.grant_id.clone(),
        executor_id: command.execution.executor_id.clone(),
        executor_revision: command.execution.executor_revision.clone(),
        terminal_classification: classification,
        result_digest,
    })
}
