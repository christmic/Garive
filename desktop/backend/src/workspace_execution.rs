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
    ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, LocalF0Governance,
    LocalGovernedExecution, LocalGovernedExecutionFactory, LocalWorkerError, PreparedExecution,
    SafetyDecisionV1, SafetyDisposition, SafetyEvaluation, SafetyFuture, SafetyInteraction,
    SafetyPort, SandboxAdmission, SandboxAdmissionPort, SandboxAdmissionRequest, SandboxBindingV1,
    SqliteLedger,
};
use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, EffectReceipt, ExecutionCapability,
    ExecutionFact, ExecutionRequirements, InteractionKind, InvocationAccessSet, PreparationError,
    PreparedToolCall, ReceiptId, ReplayClass, ResourceAccess, SandboxControl,
    SandboxRequirementsV1, TerminalClassification, ToolAccessPolicyV1, ToolAccessResolver,
    ToolCatalog, ToolDefinition, ToolIntent,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{workspace::WorkspaceWriteRoot, DesktopWorkspaceService};

/// Exact immutable revision installed in the Desktop Agent snapshot.
pub const DESKTOP_WRITE_TOOL_REVISION: &str = "desktop.workspace.write-file.v1";
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
        let access_policy = write_access_policy()?;
        let sandbox_requirements = write_sandbox_requirements()?;
        let definition = desktop_workspace_tool_definition()?;
        let tool_revision = definition.revision().to_owned();
        let preparation = WorkspacePreparation(
            ToolCatalog::new([definition.clone()])
                .map_err(|_| LocalWorkerError::InvalidComposition)?,
        );
        Ok(LocalGovernedExecution {
            capabilities: AgentToolCapabilities {
                definitions: vec![definition.clone()],
            },
            authority: Box::new(WorkspaceAuthority {
                workspaces: self.workspaces.clone(),
                owner_window: self.owner_window.clone(),
                snapshot: snapshot.clone(),
                tool_revision: tool_revision.clone(),
            }),
            executor: Box::new(WorkspaceExecutor {
                workspaces: self.workspaces.clone(),
                owner_window: self.owner_window.clone(),
                attachments: snapshot.attachments.clone(),
                tool_revision: tool_revision.clone(),
            }),
            f0: LocalF0Governance {
                preparation: Box::new(preparation),
                recovery_content: Box::new(NoWorkspaceRecoveryContent),
                safety: Box::new(WorkspaceSafety {
                    snapshot: snapshot.clone(),
                    requirements: definition.requirements().clone(),
                    policy_revision: tool_revision.clone(),
                }),
                sandbox: Box::new(WorkspaceSandbox {
                    access_policy,
                    enforcement: sandbox_requirements,
                    executor_revision: tool_revision.clone(),
                    policy_revision: tool_revision.clone(),
                }),
                context: garive_runtime::F0GovernanceContext {
                    actor_authority_reference: format!("desktop-window:{}", self.owner_window),
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: tool_revision,
                },
            },
        })
    }
}

struct WorkspacePreparation(ToolCatalog);

impl garive_core::ToolPreparationPort for WorkspacePreparation {
    fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.0.prepare_v3(intent, &WorkspaceAccessResolver)
    }
}

struct WorkspaceAccessResolver;

impl ToolAccessResolver for WorkspaceAccessResolver {
    fn revision(&self) -> &str {
        "desktop.workspace.write-access.v1"
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        let workspace = arguments["workspace_id"].as_str().unwrap_or_default();
        let artifact = arguments["artifact_name"].as_str().unwrap_or_default();
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            format!("{workspace}/{artifact}"),
            AccessMode::Write,
        )?])
    }
}

struct NoWorkspaceRecoveryContent;

impl garive_runtime::F0RecoveryContentPort for NoWorkspaceRecoveryContent {
    fn resolve(&mut self, _: &str) -> Result<String, garive_runtime::F0RecoveryError> {
        Err(garive_runtime::F0RecoveryError::ContentUnavailable)
    }
}

struct WorkspaceSafety {
    snapshot: ExecutionAuthoritySnapshot,
    requirements: ExecutionRequirements,
    policy_revision: String,
}

impl SafetyPort for WorkspaceSafety {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async move {
            let prepared = request.prepared_digest();
            let disposition = if self.snapshot.denied_digests.contains(prepared) {
                SafetyDisposition::Deny
            } else if self.snapshot.approved_digests.contains(prepared) {
                SafetyDisposition::Allow
            } else {
                SafetyDisposition::InteractionRequired
            };
            let constraints = (disposition == SafetyDisposition::Allow)
                .then(|| hex_digest(format!("{}:{prepared}", self.policy_revision).as_bytes()));
            let safe_code = match disposition {
                SafetyDisposition::Allow => None,
                SafetyDisposition::Deny => Some("safety_denied".into()),
                SafetyDisposition::InteractionRequired => {
                    Some("safety_interaction_required".into())
                }
            };
            Ok(SafetyEvaluation {
                decision: SafetyDecisionV1::new(
                    format!("workspace-safety-{}", request.request_id()),
                    disposition,
                    request.invocation_id().clone(),
                    prepared,
                    constraints,
                    self.policy_revision.clone(),
                    safe_code,
                )
                .map_err(|_| garive_runtime::GovernedRuntimePortError::InvalidBinding)?,
                granted_requirements: (disposition == SafetyDisposition::Allow)
                    .then(|| self.requirements.clone()),
                interaction: (disposition == SafetyDisposition::InteractionRequired).then(|| {
                    SafetyInteraction {
                        kind: InteractionKind::Approval,
                        prompt: json!({"schema_version":1,"title_key":"workspace.write.approval.title","message_text":"Create one new file in the attached Workspace.","action_label_key":"approval.allow_once","cancel_label_key":"approval.deny"}),
                        response_schema: json!({"type":"boolean"}),
                        expiry_code: "turn_deadline".into(),
                    }
                }),
            })
        })
    }
}

struct WorkspaceSandbox {
    access_policy: ToolAccessPolicyV1,
    enforcement: SandboxRequirementsV1,
    executor_revision: String,
    policy_revision: String,
}

impl SandboxAdmissionPort for WorkspaceSandbox {
    fn admit(
        &mut self,
        _: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, garive_runtime::GovernedRuntimePortError> {
        let nonce = Uuid::new_v4();
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                format!("workspace-binding-{nonce}"),
                "desktop-workspace-set",
                "desktop.workspace.atomic-create",
                self.executor_revision.clone(),
                self.policy_revision.clone(),
                self.access_policy.clone(),
                self.enforcement.clone(),
            )
            .map_err(|_| garive_runtime::GovernedRuntimePortError::InvalidBinding)?,
            effective_limits_digest: self
                .enforcement
                .digest()
                .map_err(|_| garive_runtime::GovernedRuntimePortError::InvalidBinding)?,
            preflight_id: format!("workspace-preflight-{nonce}"),
            dispatch_attempt_id: format!("workspace-dispatch-{nonce}"),
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
    #[serde(rename = "display_name")]
    _display_name: String,
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
                "workspace.detached" => {
                    let workspace_id = value
                        .get("workspace_id")
                        .and_then(Value::as_str)
                        .ok_or(LocalWorkerError::DurabilityUnavailable)?;
                    output.attachments.remove(workspace_id);
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
    artifact_name: String,
    content_utf8: String,
}

struct WorkspaceAuthority {
    workspaces: DesktopWorkspaceService,
    owner_window: String,
    snapshot: ExecutionAuthoritySnapshot,
    tool_revision: String,
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
            if attachment.access != "read_write"
                || self
                    .workspaces
                    .resolve_write_root(
                        &arguments.workspace_id,
                        attachment.grant_revision,
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
                    constraints_digest: binding_digest(&arguments, attachment.grant_revision),
                    authority_revision: self.tool_revision.clone(),
                });
            }
            Ok(AuthorityDecision::InteractionRequired {
                kind: InteractionKind::Approval,
                prompt: json!({
                    "schema_version":1,
                    "title_key":"workspace.write.approval.title",
                    "message_text":"Create one new file in the attached Workspace.",
                    "action_label_key":"approval.allow_once",
                    "cancel_label_key":"approval.deny",
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
    tool_revision: String,
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
            executor_revision: self.tool_revision.clone(),
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
                "artifact_contract":"garive.artifact.v1",
                "artifact_id":format!("artifact-{}", &hex_digest(format!("{}:{}:{}", arguments.workspace_id, arguments.artifact_name, result.digest).as_bytes())[..32]),
                "artifact_revision":1,
                "workspace_id":arguments.workspace_id,
                "grant_revision":self.attachments[&arguments.workspace_id].grant_revision,
                "display_name":arguments.artifact_name,
                "byte_size":result.byte_size,
                "content_digest":result.digest,
                "kind":artifact_kind(&arguments.artifact_name),
                "mime_type":artifact_mime(&arguments.artifact_name),
                "verification":"not_run",
                "preview":if artifact_kind(&arguments.artifact_name) == "text" { "text" } else { "unavailable" },
                "revealable":true,
                "exportable":true,
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
        if attachment.access != "read_write" {
            return Err("workspace_write_not_authorized".into());
        }
        self.workspaces
            .resolve_write_root(
                &arguments.workspace_id,
                attachment.grant_revision,
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
    {
        return Err("invalid".into());
    }
    Ok(arguments)
}

/// Constructs the exact Tool definition admitted by the Desktop Agent snapshot.
pub fn desktop_workspace_tool_definition() -> Result<ToolDefinition, LocalWorkerError> {
    ToolDefinition::new_v3(
        "write_file",
        DESKTOP_WRITE_TOOL_REVISION,
        "Create one new approved artifact at the root of an attached writable Workspace.",
        json!({
            "type":"object",
            "properties":{
                "workspace_id":{"type":"string","minLength":1,"maxLength":64},
                "artifact_name":{"type":"string","minLength":1,"maxLength":MAX_ARTIFACT_NAME_BYTES},
                "content_utf8":{"type":"string","maxLength":MAX_ARTIFACT_BYTES}
            },
            "required":["workspace_id","artifact_name","content_utf8"],
            "additionalProperties":false
        }),
        ExecutionRequirements::new([ExecutionCapability::FilesystemWrite], 5_000, 4_096)
            .map_err(|_| LocalWorkerError::InvalidComposition)?,
        ReplayClass::NeverReplay,
        write_access_policy()?,
        "desktop.workspace.write-access.v1",
        write_sandbox_requirements()?,
    )
    .map_err(|_| LocalWorkerError::InvalidComposition)
}

fn write_access_policy() -> Result<ToolAccessPolicyV1, LocalWorkerError> {
    ToolAccessPolicyV1::new(
        "desktop.workspace.write-policy.v1",
        [AccessPolicyEntry::new(".", [AccessMode::Write])
            .map_err(|_| LocalWorkerError::InvalidComposition)?],
        [],
        [],
        [],
        1,
        4_096,
    )
    .map_err(|_| LocalWorkerError::InvalidComposition)
}

fn write_sandbox_requirements() -> Result<SandboxRequirementsV1, LocalWorkerError> {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemWrite],
        [
            SandboxControl::FilesystemScope,
            SandboxControl::SymlinkContainment,
            SandboxControl::ResourceLimits,
        ],
        None,
        8,
    )
    .map_err(|_| LocalWorkerError::InvalidComposition)
}

fn binding_digest(arguments: &WriteArguments, grant_revision: u64) -> String {
    hex_digest(
        format!(
            "{}:{}:{}:{}",
            arguments.workspace_id,
            grant_revision,
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

fn artifact_mime(name: &str) -> &'static str {
    match PathBuf::from(name)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("md") => "text/markdown",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        _ => "application/octet-stream",
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    fn arguments(name: &str, content: &str) -> WriteArguments {
        WriteArguments {
            workspace_id: "workspace-test".into(),
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
