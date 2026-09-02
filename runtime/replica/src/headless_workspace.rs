//! Explicit Workspace governance used by the headless Runtime composition.

use std::collections::BTreeMap;

use garive_tools::{
    ExecutionRequirements, PreparedToolCall, SandboxRequirementsV1, ToolAccessPolicyV1,
    ToolInvocationId,
};
use sha2::{Digest, Sha256};

use crate::{
    t1_dispatch_attempt_id, AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest,
    CommittedTurn, F0GovernanceContext, F0RecoveryContentPort, F0RecoveryError,
    GovernedRuntimePortError, LocalF0Governance, LocalGovernedExecution,
    LocalGovernedExecutionFactory, LocalWorkerError, SafetyDecisionV1, SafetyDisposition,
    SafetyEvaluation, SafetyFuture, SafetyPort, SandboxAdmission, SandboxAdmissionPort,
    SandboxAdmissionRequest, SandboxBindingV1, T1RuntimeExecution, T1WorkspaceRuntimeConfig,
    T1_PATCH_EXECUTOR_ID, T1_WORKSPACE_EXECUTOR_ID,
};

/// Stable Safety/authority policy revision for explicitly bound headless workspaces.
pub const HEADLESS_WORKSPACE_POLICY_REVISION: &str = "headless.workspace.policy.v1";
/// Stable concrete executor revision for headless workspace tools.
pub const HEADLESS_WORKSPACE_EXECUTOR_REVISION: &str = "headless.workspace.executor.v1";

/// Creates a fresh governed Workspace execution for every committed Turn.
pub struct HeadlessWorkspaceExecutionFactory {
    config: T1WorkspaceRuntimeConfig,
    workspace_capability_id: String,
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
            config,
            workspace_capability_id,
        })
    }
}

impl LocalGovernedExecutionFactory for HeadlessWorkspaceExecutionFactory {
    fn create(&self, _: &CommittedTurn) -> Result<LocalGovernedExecution, LocalWorkerError> {
        let execution = self
            .config
            .build()
            .map_err(|_| LocalWorkerError::InvalidComposition)?;
        let profiles = enforcement_profiles(&execution)?;
        let requirements = execution
            .capabilities()
            .definitions
            .iter()
            .map(|definition| {
                (
                    definition.name().to_owned(),
                    definition.requirements().clone(),
                )
            })
            .collect();
        let (capabilities, preparation, executor) = execution.into_parts();
        Ok(LocalGovernedExecution {
            capabilities,
            authority: Box::new(WorkspaceAuthority),
            executor,
            f0: LocalF0Governance {
                preparation,
                recovery_content: Box::new(NoReferencedContent),
                safety: Box::new(WorkspaceSafety { requirements }),
                sandbox: Box::new(WorkspaceSandbox {
                    profiles,
                    workspace_capability_id: self.workspace_capability_id.clone(),
                }),
                context: F0GovernanceContext {
                    actor_authority_reference: "headless-loopback-operator".into(),
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
    executor_id: &'static str,
    executor_revision: String,
}

struct WorkspaceSandbox {
    profiles: BTreeMap<String, EnforcementProfile>,
    workspace_capability_id: String,
}

impl SandboxAdmissionPort for WorkspaceSandbox {
    fn admit(
        &mut self,
        request: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, GovernedRuntimePortError> {
        let profile = self
            .profiles
            .get(request.safety_request.tool_name())
            .ok_or(GovernedRuntimePortError::InvalidBinding)?;
        let invocation = request.safety_request.invocation_id();
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                stable_id("binding", invocation),
                self.workspace_capability_id.clone(),
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
            dispatch_attempt_id: t1_dispatch_attempt_id(profile.executor_id, invocation)
                .ok_or(GovernedRuntimePortError::InvalidBinding)?,
        })
    }
}

fn enforcement_profiles(
    execution: &T1RuntimeExecution,
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
                    executor_id,
                    executor_revision: binding.executor_revision().to_owned(),
                },
            ))
        })
        .collect()
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
