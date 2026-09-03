//! Explicit Workspace governance used by the headless Runtime composition.

use std::{collections::BTreeMap, path::PathBuf};

use garive_core::ToolPreparationPort;
use garive_multiagent::{
    CollaborationToolCatalogue, COLLECT_DELEGATIONS_TOOL, DELEGATE_TOOL, FORK_SELF_TOOL,
    MESSAGE_AGENT_TOOL,
};
use garive_tools::{
    EffectReceipt, ExecutionRequirements, InvocationGrant, PreparationError, PreparedToolCall,
    SandboxRequirementsV1, ToolAccessPolicyV1, ToolIntent, ToolInvocationId,
};
use sha2::{Digest, Sha256};

use crate::{
    autonomous_collaboration::{
        collaboration_dispatch_attempt_id, validate_collaboration_admission,
    },
    t1_dispatch_attempt_id, AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest,
    AutonomousCollaborationExecutor, AutonomousCollaborationOutbox,
    AutonomousCollaborationPreparation, CommittedTurn, ExecutorDispatch, ExecutorDispatchError,
    ExecutorFuture, ExecutorPort, ExecutorRecoveryRequest, F0GovernanceContext,
    F0RecoveryContentPort, F0RecoveryError, GovernedRuntimePortError, LocalF0Governance,
    LocalGovernedExecution, LocalGovernedExecutionFactory, LocalWorkerError, PreparedExecution,
    SafetyDecisionV1, SafetyDisposition, SafetyEvaluation, SafetyFuture, SafetyPort,
    SandboxAdmission, SandboxAdmissionPort, SandboxAdmissionRequest, SandboxBindingV1,
    T1RuntimeExecution, T1WorkspaceRuntimeConfig, COLLABORATION_EXECUTOR_ID,
    COLLABORATION_EXECUTOR_REVISION, COLLABORATION_POLICY_REVISION, T1_PATCH_EXECUTOR_ID,
    T1_WORKSPACE_EXECUTOR_ID,
};

/// Stable Safety/authority policy revision for explicitly bound headless workspaces.
pub const HEADLESS_WORKSPACE_POLICY_REVISION: &str = "headless.workspace.policy.v1";
/// Stable concrete executor revision for headless workspace tools.
pub const HEADLESS_WORKSPACE_EXECUTOR_REVISION: &str = "headless.workspace.executor.v1";

/// Creates a fresh governed Workspace execution for every committed Turn.
pub struct HeadlessWorkspaceExecutionFactory {
    config: Option<T1WorkspaceRuntimeConfig>,
    workspace_capability_id: String,
    collaboration: Option<HeadlessCollaborationBinding>,
}

struct HeadlessCollaborationBinding {
    database_path: PathBuf,
    outbox: AutonomousCollaborationOutbox,
}

impl HeadlessWorkspaceExecutionFactory {
    /// Binds one validated workspace tool configuration and stable capability identity.
    pub fn new(
        config: T1WorkspaceRuntimeConfig,
        workspace_capability_id: impl Into<String>,
    ) -> Result<Self, LocalWorkerError> {
        let workspace_capability_id = workspace_capability_id.into();
        if workspace_capability_id.is_empty() {
            return Err(LocalWorkerError::InvalidComposition);
        }
        Ok(Self {
            config: Some(config),
            workspace_capability_id,
            collaboration: None,
        })
    }

    /// Constructs a collaboration-only execution surface without Workspace authority.
    pub fn collaboration_only(
        database_path: impl Into<PathBuf>,
        outbox: AutonomousCollaborationOutbox,
    ) -> Result<Self, LocalWorkerError> {
        let database_path = database_path.into();
        if database_path.as_os_str().is_empty() {
            return Err(LocalWorkerError::InvalidComposition);
        }
        Ok(Self {
            config: None,
            workspace_capability_id: "headless-collaboration".into(),
            collaboration: Some(HeadlessCollaborationBinding {
                database_path,
                outbox,
            }),
        })
    }

    /// Adds Agent-originated collaboration to this Workspace execution surface.
    pub fn with_autonomous_collaboration(
        mut self,
        database_path: impl Into<PathBuf>,
        outbox: AutonomousCollaborationOutbox,
    ) -> Result<Self, LocalWorkerError> {
        let database_path = database_path.into();
        if database_path.as_os_str().is_empty() {
            return Err(LocalWorkerError::InvalidComposition);
        }
        self.collaboration = Some(HeadlessCollaborationBinding {
            database_path,
            outbox,
        });
        Ok(self)
    }
}

impl LocalGovernedExecutionFactory for HeadlessWorkspaceExecutionFactory {
    fn create(
        &self,
        committed: &CommittedTurn,
    ) -> Result<LocalGovernedExecution, LocalWorkerError> {
        let workspace = self
            .config
            .as_ref()
            .map(|config| {
                config
                    .build()
                    .map_err(|_| LocalWorkerError::InvalidComposition)
            })
            .transpose()?;
        let mut profiles = workspace
            .as_ref()
            .map(|execution| enforcement_profiles(execution, &self.workspace_capability_id))
            .transpose()?
            .unwrap_or_default();
        let mut definitions = workspace
            .as_ref()
            .map(|execution| execution.capabilities().definitions.clone())
            .unwrap_or_default();
        let collaboration = self
            .collaboration
            .as_ref()
            .map(|binding| {
                let catalogue = CollaborationToolCatalogue::new(COLLABORATION_POLICY_REVISION)
                    .map_err(|_| LocalWorkerError::InvalidComposition)?;
                for definition in catalogue.definitions() {
                    profiles.insert(
                        definition.name().into(),
                        EnforcementProfile {
                            access: definition
                                .access_policy()
                                .cloned()
                                .ok_or(LocalWorkerError::InvalidComposition)?,
                            sandbox: definition
                                .sandbox_requirements()
                                .cloned()
                                .ok_or(LocalWorkerError::InvalidComposition)?,
                            capability_id: "headless-collaboration".into(),
                            executor_id: COLLABORATION_EXECUTOR_ID,
                            executor_revision: COLLABORATION_EXECUTOR_REVISION.into(),
                        },
                    );
                }
                definitions.extend_from_slice(catalogue.definitions());
                AutonomousCollaborationExecutor::new(
                    &binding.database_path,
                    committed,
                    binding.outbox.clone(),
                )
            })
            .transpose()?;
        let actor_authority_reference = collaboration
            .as_ref()
            .map(|executor| format!("agent:{}", executor.origin().agent_instance_id()))
            .unwrap_or_else(|| "headless-loopback-operator".into());
        let collaboration_validation = self
            .collaboration
            .as_ref()
            .zip(collaboration.as_ref())
            .map(|(binding, executor)| (binding.database_path.clone(), executor.origin().clone()));
        definitions.sort_by(|left, right| left.name().cmp(right.name()));
        let requirements = definitions
            .iter()
            .map(|definition| {
                (
                    definition.name().to_owned(),
                    definition.requirements().clone(),
                )
            })
            .collect();
        let (workspace_preparation, workspace_executor) = match workspace {
            Some(execution) => {
                let (_, preparation, executor) = execution.into_parts();
                (Some(preparation), Some(executor))
            }
            None => (None, None),
        };
        let collaboration_preparation = self
            .collaboration
            .as_ref()
            .map(|_| AutonomousCollaborationPreparation::new())
            .transpose()?;
        let preparation: Box<dyn ToolPreparationPort> = Box::new(HeadlessPreparationRouter {
            workspace: workspace_preparation,
            collaboration: collaboration_preparation,
        });
        let executor: Box<dyn ExecutorPort> = Box::new(HeadlessExecutorRouter {
            workspace: workspace_executor,
            collaboration: collaboration.map(|value| Box::new(value) as Box<dyn ExecutorPort>),
        });
        Ok(LocalGovernedExecution {
            capabilities: garive_core::AgentToolCapabilities { definitions },
            authority: Box::new(WorkspaceAuthority),
            executor,
            f0: LocalF0Governance {
                preparation,
                recovery_content: Box::new(NoReferencedContent),
                safety: Box::new(WorkspaceSafety { requirements }),
                sandbox: Box::new(WorkspaceSandbox {
                    profiles,
                    collaboration_validation,
                }),
                context: F0GovernanceContext {
                    actor_authority_reference,
                    goal_reference: None,
                    plan_reference: None,
                    effective_policy_revision: HEADLESS_WORKSPACE_POLICY_REVISION.into(),
                },
            },
        })
    }
}

struct WorkspaceAuthority;

impl AuthorityPort for WorkspaceAuthority {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            Ok(AuthorityDecision::Approve {
                granted_requirements: request.prepared.requirements().clone(),
                constraints_digest: binding_digest(request.prepared),
                authority_revision: HEADLESS_WORKSPACE_POLICY_REVISION.into(),
            })
        })
    }
}

struct WorkspaceSafety {
    requirements: BTreeMap<String, ExecutionRequirements>,
}

impl SafetyPort for WorkspaceSafety {
    fn decide<'a>(&'a mut self, request: &'a crate::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async move {
            let requirements = self
                .requirements
                .get(request.tool_name())
                .ok_or(GovernedRuntimePortError::InvalidBinding)?;
            let constraints = format!(
                "{:x}",
                Sha256::digest(
                    format!(
                        "{}:{}",
                        HEADLESS_WORKSPACE_POLICY_REVISION,
                        request.prepared_digest()
                    )
                    .as_bytes()
                )
            );
            Ok(SafetyEvaluation {
                decision: SafetyDecisionV1::new(
                    format!("headless-workspace-safety-{}", request.request_id()),
                    SafetyDisposition::Allow,
                    request.invocation_id().clone(),
                    request.prepared_digest(),
                    Some(constraints),
                    HEADLESS_WORKSPACE_POLICY_REVISION,
                    None,
                )
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
                granted_requirements: Some(requirements.clone()),
                interaction: None,
            })
        })
    }
}

#[derive(Clone)]
struct EnforcementProfile {
    access: ToolAccessPolicyV1,
    sandbox: SandboxRequirementsV1,
    capability_id: String,
    executor_id: &'static str,
    executor_revision: String,
}

struct WorkspaceSandbox {
    profiles: BTreeMap<String, EnforcementProfile>,
    collaboration_validation: Option<(PathBuf, crate::AutonomousCollaborationOrigin)>,
}

impl SandboxAdmissionPort for WorkspaceSandbox {
    fn admit(
        &mut self,
        request: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, GovernedRuntimePortError> {
        if collaboration_tool(request.prepared.tool_name()) {
            let (database_path, origin) = self
                .collaboration_validation
                .as_ref()
                .ok_or(GovernedRuntimePortError::InvalidBinding)?;
            validate_collaboration_admission(database_path, origin, request.prepared)
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?;
        }
        let profile = self
            .profiles
            .get(request.safety_request.tool_name())
            .ok_or(GovernedRuntimePortError::InvalidBinding)?;
        let invocation = request.safety_request.invocation_id();
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                stable_id("binding", invocation),
                profile.capability_id.clone(),
                profile.executor_id,
                profile.executor_revision.clone(),
                HEADLESS_WORKSPACE_POLICY_REVISION,
                profile.access.clone(),
                profile.sandbox.clone(),
            )
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            effective_limits_digest: profile
                .sandbox
                .digest()
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            preflight_id: stable_id("preflight", invocation),
            dispatch_attempt_id: if profile.executor_id == COLLABORATION_EXECUTOR_ID {
                collaboration_dispatch_attempt_id(invocation)
            } else {
                t1_dispatch_attempt_id(profile.executor_id, invocation)
                    .ok_or(GovernedRuntimePortError::InvalidBinding)?
            },
        })
    }
}

fn enforcement_profiles(
    execution: &T1RuntimeExecution,
    capability_id: &str,
) -> Result<BTreeMap<String, EnforcementProfile>, LocalWorkerError> {
    execution
        .capabilities()
        .definitions
        .iter()
        .map(|definition| {
            let binding = execution
                .executor_binding(definition.name())
                .ok_or(LocalWorkerError::InvalidComposition)?;
            let executor_id = match binding.executor_id() {
                T1_WORKSPACE_EXECUTOR_ID => T1_WORKSPACE_EXECUTOR_ID,
                T1_PATCH_EXECUTOR_ID => T1_PATCH_EXECUTOR_ID,
                _ => return Err(LocalWorkerError::InvalidComposition),
            };
            Ok((
                definition.name().to_owned(),
                EnforcementProfile {
                    access: definition
                        .access_policy()
                        .cloned()
                        .ok_or(LocalWorkerError::InvalidComposition)?,
                    sandbox: definition
                        .sandbox_requirements()
                        .cloned()
                        .ok_or(LocalWorkerError::InvalidComposition)?,
                    capability_id: capability_id.into(),
                    executor_id,
                    executor_revision: binding.executor_revision().to_owned(),
                },
            ))
        })
        .collect()
}

struct HeadlessPreparationRouter {
    workspace: Option<Box<dyn ToolPreparationPort>>,
    collaboration: Option<AutonomousCollaborationPreparation>,
}

impl ToolPreparationPort for HeadlessPreparationRouter {
    fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        if collaboration_tool(intent.tool_name()) {
            self.collaboration
                .as_ref()
                .expect("collaboration Tool is installed with its preparation port")
                .prepare(intent)
        } else if let Some(workspace) = &self.workspace {
            workspace.prepare(intent)
        } else {
            self.workspace
                .as_ref()
                .map(|workspace| workspace.prepare(intent))
                .unwrap_or_else(|| {
                    self.collaboration
                        .as_ref()
                        .expect("one preparation port is installed")
                        .prepare(intent)
                })
        }
    }
}

struct HeadlessExecutorRouter {
    workspace: Option<Box<dyn ExecutorPort>>,
    collaboration: Option<Box<dyn ExecutorPort>>,
}

impl ExecutorPort for HeadlessExecutorRouter {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        if collaboration_tool(prepared.tool_name()) {
            self.collaboration
                .as_deref_mut()
                .ok_or_else(|| "collaboration executor unavailable".to_owned())?
                .prepare(invocation_id, prepared, grant)
        } else {
            self.workspace
                .as_deref_mut()
                .ok_or_else(|| "Workspace executor unavailable".to_owned())?
                .prepare(invocation_id, prepared, grant)
        }
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        if command.execution.executor_id == COLLABORATION_EXECUTOR_ID {
            match self.collaboration.as_deref_mut() {
                Some(executor) => executor.dispatch(command),
                None => Box::pin(async { Err(ExecutorDispatchError::ReceiptInvalid) }),
            }
        } else {
            match self.workspace.as_deref_mut() {
                Some(executor) => executor.dispatch(command),
                None => Box::pin(async { Err(ExecutorDispatchError::ReceiptInvalid) }),
            }
        }
    }

    fn acknowledge_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        if receipt.executor_id == COLLABORATION_EXECUTOR_ID {
            self.collaboration
                .as_deref_mut()
                .ok_or(ExecutorDispatchError::ReceiptInvalid)?
                .acknowledge_receipt(invocation_id, receipt)
        } else {
            self.workspace
                .as_deref_mut()
                .ok_or(ExecutorDispatchError::ReceiptInvalid)?
                .acknowledge_receipt(invocation_id, receipt)
        }
    }

    fn reconcile_started_loss(
        &mut self,
        request: ExecutorRecoveryRequest<'_>,
    ) -> Result<(), ExecutorDispatchError> {
        if request.executor_id == COLLABORATION_EXECUTOR_ID {
            self.collaboration
                .as_deref_mut()
                .ok_or(ExecutorDispatchError::ReceiptInvalid)?
                .reconcile_started_loss(request)
        } else {
            self.workspace
                .as_deref_mut()
                .ok_or(ExecutorDispatchError::ReceiptInvalid)?
                .reconcile_started_loss(request)
        }
    }
}

fn collaboration_tool(name: &str) -> bool {
    matches!(
        name,
        MESSAGE_AGENT_TOOL | DELEGATE_TOOL | FORK_SELF_TOOL | COLLECT_DELEGATIONS_TOOL
    )
}

struct NoReferencedContent;

impl F0RecoveryContentPort for NoReferencedContent {
    fn resolve(&mut self, _: &str) -> Result<String, F0RecoveryError> {
        Err(F0RecoveryError::ContentUnavailable)
    }
}

fn binding_digest(prepared: &PreparedToolCall) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}:{}",
                HEADLESS_WORKSPACE_POLICY_REVISION,
                prepared.input_digest()
            )
            .as_bytes()
        )
    )
}

fn stable_id(kind: &str, invocation: &ToolInvocationId) -> String {
    format!(
        "headless-workspace-{kind}-{:x}",
        Sha256::digest(format!("{}:{kind}", invocation.as_str()).as_bytes())
    )
}
