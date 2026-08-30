use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Write,
    path::PathBuf,
};

use garive_core::AgentToolCapabilities;
use garive_ledger::CanonicalPayload;
use garive_runtime::{
    AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest, CommittedTurn,
    ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, LocalGovernedExecution,
    LocalGovernedExecutionFactory, LocalWorkerError, PreparedExecution, SqliteLedger,
};
use garive_tools::{
    EffectReceipt, ExecutionCapability, ExecutionFact, ExecutionRequirements, InteractionKind,
    ReceiptId, ReplayClass, TerminalClassification, ToolDefinition,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{workspace::WorkspaceWriteRoot, DesktopWorkspaceService};

const WRITE_TOOL_REVISION: &str = "desktop.workspace.write-file.v1";
const MAX_ARTIFACT_NAME_BYTES: usize = 128;
const MAX_ARTIFACT_BYTES: usize = 256 * 1_024;

/// Creates one isolated governed Workspace execution surface per committed Turn.
pub struct DesktopWorkspaceExecutionFactory {
    database_path: PathBuf,
    workspaces: DesktopWorkspaceService,
    owner_window: String,
}

impl DesktopWorkspaceExecutionFactory {
    /// Binds governed executions to one durable ledger and shared Workspace registry.
    pub fn new(
        database_path: PathBuf,
        workspaces: DesktopWorkspaceService,
        owner_window: impl Into<String>,
    ) -> Result<Self, LocalWorkerError> {
        let owner_window = owner_window.into();
        if database_path.as_os_str().is_empty() || owner_window.is_empty() {
            return Err(LocalWorkerError::InvalidComposition);
        }
        Ok(Self {
            database_path,
            workspaces,
            owner_window,
        })
    }
}

impl LocalGovernedExecutionFactory for DesktopWorkspaceExecutionFactory {
    fn create(
        &self,
        committed: &CommittedTurn,
    ) -> Result<LocalGovernedExecution, LocalWorkerError> {
        let ledger = SqliteLedger::open(&self.database_path)
            .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
        let facts = ledger
            .read_facts(&committed.session_id, 0, committed.committed_position, None)
            .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
        let snapshot = ExecutionAuthoritySnapshot::from_facts(&facts)?;
        Ok(LocalGovernedExecution {
            capabilities: AgentToolCapabilities {
                definitions: vec![write_definition()?],
            },
            authority: Box::new(WorkspaceAuthority {
                workspaces: self.workspaces.clone(),
                owner_window: self.owner_window.clone(),
                snapshot: snapshot.clone(),
            }),
            executor: Box::new(WorkspaceExecutor {
                workspaces: self.workspaces.clone(),
                owner_window: self.owner_window.clone(),
                attachments: snapshot.attachments,
            }),
        })
    }
}

#[derive(Clone, Default)]
struct ExecutionAuthoritySnapshot {
    attachments: BTreeMap<String, WorkspaceAttachment>,
    approved_digests: BTreeSet<String>,
    denied_digests: BTreeSet<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceAttachment {
    #[serde(rename = "command_id")]
    _command_id: String,
    workspace_id: String,
    display_name: String,
    grant_revision: u64,
    access: String,
}

impl ExecutionAuthoritySnapshot {
    fn from_facts(facts: &[garive_ledger::DurableFact]) -> Result<Self, LocalWorkerError> {
        let mut output = Self::default();
        for fact in facts {
            let value: Value = serde_json::from_str(fact.payload.as_json())
                .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
            match fact.kind.as_str() {
                "workspace.attached" => {
                    let attachment: WorkspaceAttachment = serde_json::from_value(value)
                        .map_err(|_| LocalWorkerError::DurabilityUnavailable)?;
                    output
                        .attachments
                        .insert(attachment.workspace_id.clone(), attachment);
                }
                "interaction.resolved" => {
                    let digest = value.get("prepared_digest").and_then(Value::as_str);
                    let response = value
                        .pointer("/response/inline_utf8")
                        .and_then(Value::as_str);
                    if let (Some(digest), Some(response)) = (digest, response) {
                        match response {
                            "true" => {
                                output.approved_digests.insert(digest.to_owned());
                            }
                            "false" => {
                                output.denied_digests.insert(digest.to_owned());
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(output)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArguments {
    workspace_id: String,
    grant_revision: u64,
    artifact_name: String,
    content_utf8: String,
}

struct WorkspaceAuthority {
    workspaces: DesktopWorkspaceService,
    owner_window: String,
    snapshot: ExecutionAuthoritySnapshot,
}

impl AuthorityPort for WorkspaceAuthority {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            let Ok(arguments) = arguments(request.prepared.normalized_arguments()) else {
                return Ok(AuthorityDecision::Deny {
                    safe_details: Some("invalid_workspace_write".into()),
                });
            };
            let Some(attachment) = self.snapshot.attachments.get(&arguments.workspace_id) else {
                return Ok(denied("workspace_not_attached"));
            };
            if attachment.grant_revision != arguments.grant_revision
                || attachment.access != "read_write"
                || self
                    .workspaces
                    .resolve_write_root(
                        &arguments.workspace_id,
                        arguments.grant_revision,
                        &self.owner_window,
                    )
                    .is_err()
            {
                return Ok(denied("workspace_write_not_authorized"));
            }
            if self
                .snapshot
                .denied_digests
                .contains(request.prepared.input_digest())
            {
                return Ok(denied("user_denied"));
            }
            if self
                .snapshot
                .approved_digests
                .contains(request.prepared.input_digest())
            {
                return Ok(AuthorityDecision::Approve {
                    granted_requirements: request.prepared.requirements().clone(),
                    constraints_digest: binding_digest(&arguments),
                    authority_revision: WRITE_TOOL_REVISION.into(),
                });
            }
            Ok(AuthorityDecision::InteractionRequired {
                kind: InteractionKind::Approval,
                prompt: json!({
                    "action":"create_workspace_artifact",
                    "workspace":attachment.display_name,
                    "artifact_name":arguments.artifact_name,
                    "byte_size":arguments.content_utf8.len(),
                    "content_digest":hex_digest(arguments.content_utf8.as_bytes()),
                }),
                response_schema: json!({"type":"boolean"}),
                expiry_code: "turn_deadline".into(),
            })
        })
    }
}

fn denied(details: &str) -> AuthorityDecision {
    AuthorityDecision::Deny {
        safe_details: Some(details.into()),
    }
}

struct WorkspaceExecutor {
    workspaces: DesktopWorkspaceService,
    owner_window: String,
    attachments: BTreeMap<String, WorkspaceAttachment>,
}

impl ExecutorPort for WorkspaceExecutor {
    fn prepare(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        prepared: &garive_tools::PreparedToolCall,
        _: &garive_tools::InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        let arguments = arguments(prepared.normalized_arguments())?;
        self.validate(&arguments)?;
        Ok(PreparedExecution {
            executor_id: "desktop.workspace.atomic-create".into(),
            executor_revision: WRITE_TOOL_REVISION.into(),
            dispatch_attempt_id: format!("workspace-write-{}", Uuid::new_v4()),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        Box::pin(async move {
            let arguments = arguments(command.prepared.normalized_arguments())
                .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            let root = self
                .validate(&arguments)
                .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            let result = match atomic_create(&root, &arguments) {
                Ok(result) => result,
                Err(AtomicCreateError::NotCreated) => {
                    let evidence = json!({"code":"artifact_not_created"});
                    return Ok(ExecutionFact::Failed {
                        receipt: Some(effect_receipt(
                            &command,
                            &evidence,
                            TerminalClassification::Failed,
                        )?),
                        code: "artifact_not_created".into(),
                        details: None,
                        partial: None,
                    });
                }
                Err(AtomicCreateError::StateUnknown) => {
                    return Err(ExecutorDispatchError::ExecutorStateUnknown)
                }
            };
            let content = json!({
                "artifact_id":format!("artifact-{}", &hex_digest(format!("{}:{}:{}", arguments.workspace_id, arguments.artifact_name, result.digest).as_bytes())[..32]),
                "workspace_id":arguments.workspace_id,
                "grant_revision":arguments.grant_revision,
                "display_name":arguments.artifact_name,
                "byte_size":result.byte_size,
                "content_digest":result.digest,
                "kind":artifact_kind(&arguments.artifact_name),
            });
            Ok(ExecutionFact::Completed {
                receipt: Some(effect_receipt(
                    &command,
                    &content,
                    TerminalClassification::Completed,
                )?),
                content,
                truncated: false,
            })
        })
    }
}

impl WorkspaceExecutor {
    fn validate(&self, arguments: &WriteArguments) -> Result<WorkspaceWriteRoot, String> {
        let attachment = self
            .attachments
            .get(&arguments.workspace_id)
            .ok_or_else(|| "workspace_not_attached".to_owned())?;
        if attachment.grant_revision != arguments.grant_revision
            || attachment.access != "read_write"
        {
            return Err("workspace_write_not_authorized".into());
        }
        self.workspaces
            .resolve_write_root(
                &arguments.workspace_id,
                arguments.grant_revision,
                &self.owner_window,
            )
            .map_err(|_| "workspace_write_not_authorized".into())
    }
}

fn effect_receipt(
    command: &ExecutorDispatch<'_>,
    evidence: &Value,
    classification: TerminalClassification,
) -> Result<EffectReceipt, ExecutorDispatchError> {
    Ok(EffectReceipt {
        receipt_id: ReceiptId::new(command.receipt_id)
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
        invocation_id: command.invocation_id.clone(),
        prepared_digest: command.prepared.input_digest().into(),
        grant_id: command.grant.grant_id.clone(),
        executor_id: command.execution.executor_id.clone(),
        executor_revision: command.execution.executor_revision.clone(),
        terminal_classification: classification,
        result_digest: CanonicalPayload::from_value(evidence)
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
            .sha256()
            .into(),
    })
}

struct WriteResult {
    digest: String,
    byte_size: usize,
}

#[derive(Debug)]
enum AtomicCreateError {
    NotCreated,
    StateUnknown,
}

#[cfg(unix)]
fn atomic_create(
    root: &WorkspaceWriteRoot,
    arguments: &WriteArguments,
) -> Result<WriteResult, AtomicCreateError> {
    use rustix::fs::{AtFlags, Mode, OFlags};

    let temporary = format!(".garive-{}.tmp", Uuid::new_v4());
    let descriptor = rustix::fs::openat(
        root.directory(),
        temporary.as_str(),
        OFlags::CREATE | OFlags::EXCL | OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|_| AtomicCreateError::NotCreated)?;
    let mut file = File::from(descriptor);
    if file
        .write_all(arguments.content_utf8.as_bytes())
        .and_then(|()| file.sync_all())
        .is_err()
    {
        let _ = rustix::fs::unlinkat(root.directory(), temporary.as_str(), AtFlags::empty());
        return Err(AtomicCreateError::NotCreated);
    }
    if rustix::fs::linkat(
        root.directory(),
        temporary.as_str(),
        root.directory(),
        arguments.artifact_name.as_str(),
        AtFlags::empty(),
    )
    .is_err()
    {
        let _ = rustix::fs::unlinkat(root.directory(), temporary.as_str(), AtFlags::empty());
        return Err(AtomicCreateError::NotCreated);
    }
    let _ = rustix::fs::unlinkat(root.directory(), temporary.as_str(), AtFlags::empty());
    rustix::fs::fsync(root.directory()).map_err(|_| AtomicCreateError::StateUnknown)?;
    Ok(WriteResult {
        digest: hex_digest(arguments.content_utf8.as_bytes()),
        byte_size: arguments.content_utf8.len(),
    })
}

#[cfg(not(unix))]
fn atomic_create(
    _: &WorkspaceWriteRoot,
    _: &WriteArguments,
) -> Result<WriteResult, AtomicCreateError> {
    Err(AtomicCreateError::NotCreated)
}

fn arguments(value: &str) -> Result<WriteArguments, String> {
    let arguments: WriteArguments =
        serde_json::from_str(value).map_err(|_| "invalid".to_owned())?;
    let name = &arguments.artifact_name;
    if name.is_empty()
        || name.len() > MAX_ARTIFACT_NAME_BYTES
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || arguments.content_utf8.len() > MAX_ARTIFACT_BYTES
        || arguments.grant_revision == 0
    {
        return Err("invalid".into());
    }
    Ok(arguments)
}

fn write_definition() -> Result<ToolDefinition, LocalWorkerError> {
    ToolDefinition::new(
        "write_file",
        WRITE_TOOL_REVISION,
        "Create one new approved artifact at the root of an attached writable Workspace.",
        json!({
            "type":"object",
            "properties":{
                "workspace_id":{"type":"string","minLength":1,"maxLength":64},
                "grant_revision":{"type":"integer","minimum":1},
                "artifact_name":{"type":"string","minLength":1,"maxLength":MAX_ARTIFACT_NAME_BYTES},
                "content_utf8":{"type":"string","maxLength":MAX_ARTIFACT_BYTES}
            },
            "required":["workspace_id","grant_revision","artifact_name","content_utf8"],
            "additionalProperties":false
        }),
        ExecutionRequirements::new([ExecutionCapability::FilesystemWrite], 5_000, 4_096)
            .map_err(|_| LocalWorkerError::InvalidComposition)?,
        ReplayClass::NeverReplay,
    )
    .map_err(|_| LocalWorkerError::InvalidComposition)
}

fn binding_digest(arguments: &WriteArguments) -> String {
    hex_digest(
        format!(
            "{}:{}:{}:{}",
            arguments.workspace_id,
            arguments.grant_revision,
            arguments.artifact_name,
            hex_digest(arguments.content_utf8.as_bytes())
        )
        .as_bytes(),
    )
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn artifact_kind(name: &str) -> &'static str {
    match PathBuf::from(name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("md" | "txt" | "json" | "csv") => "text",
        _ => "file",
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    fn arguments(name: &str, content: &str) -> WriteArguments {
        WriteArguments {
            workspace_id: "workspace-test".into(),
            grant_revision: 2,
            artifact_name: name.into(),
            content_utf8: content.into(),
        }
    }

    #[test]
    fn descriptor_relative_create_is_atomic_and_never_overwrites() {
        let directory = tempfile::tempdir().unwrap();
        let root = WorkspaceWriteRoot {
            directory: File::open(directory.path()).unwrap(),
        };
        let request = arguments("result.md", "first");
        let receipt = atomic_create(&root, &request).unwrap();
        assert_eq!(receipt.byte_size, 5);
        assert_eq!(
            fs::read_to_string(directory.path().join("result.md")).unwrap(),
            "first"
        );

        let replacement = arguments("result.md", "second");
        assert!(matches!(
            atomic_create(&root, &replacement),
            Err(AtomicCreateError::NotCreated)
        ));
        assert_eq!(
            fs::read_to_string(directory.path().join("result.md")).unwrap(),
            "first"
        );
        assert!(fs::read_dir(directory.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".garive-")));
    }

    #[test]
    fn opened_workspace_descriptor_cannot_be_redirected_by_path_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let selected = parent.path().join("selected");
        let moved = parent.path().join("moved");
        fs::create_dir(&selected).unwrap();
        let root = WorkspaceWriteRoot {
            directory: File::open(&selected).unwrap(),
        };
        fs::rename(&selected, &moved).unwrap();
        fs::create_dir(&selected).unwrap();

        atomic_create(&root, &arguments("bound.txt", "bound")).unwrap();
        assert_eq!(
            fs::read_to_string(moved.join("bound.txt")).unwrap(),
            "bound"
        );
        assert!(!selected.join("bound.txt").exists());
    }
}
