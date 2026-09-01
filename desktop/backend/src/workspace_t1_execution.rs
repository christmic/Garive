//! Closed six-tool routing for one exact Desktop Workspace Agent execution.

use std::collections::{BTreeMap, BTreeSet};

use garive_core::{AgentToolCapabilities, ToolPreparationPort};
use garive_runtime::{
    t1_dispatch_attempt_id, AuthorityDecision, AuthorityFuture, AuthorityPort, AuthorityRequest,
    ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, ExecutorRecoveryRequest,
    GovernedRuntimePortError, LocalGovernedExecution, LocalWorkerError, SafetyDecisionV1,
    SafetyDisposition, SafetyEvaluation, SafetyFuture, SafetyInteraction, SafetyPort,
    SandboxAdmission, SandboxAdmissionPort, SandboxAdmissionRequest, SandboxBindingV1,
    T1RuntimeExecution, T1_PATCH_EXECUTOR_ID, T1_PROCESS_EXECUTOR_ID, T1_WORKSPACE_EXECUTOR_ID,
};
use garive_tools::{
    EffectReceipt, ExecutionRequirements, InteractionKind, PreparedToolCall, SandboxRequirementsV1,
    ToolAccessPolicyV1, ToolDefinition, ToolIntent, ToolInvocationId, T1_APPLY_PATCH, T1_LIST,
    T1_PROCESS_RUN, T1_READ_TEXT, T1_SEARCH_TEXT,
};
use serde_json::json;
use sha2::{Digest, Sha256};

const WRITE_TOOL: &str = "write_file";

pub(crate) struct WorkspaceT1Governance {
    pub(crate) policy_revision: String,
    pub(crate) executor_revision: String,
    pub(crate) workspace_capability_id: String,
    pub(crate) approved_digests: BTreeSet<String>,
    pub(crate) denied_digests: BTreeSet<String>,
}

pub(crate) fn extend_with_t1(
    base: LocalGovernedExecution,
    t1: T1RuntimeExecution,
    governance: WorkspaceT1Governance,
) -> Result<LocalGovernedExecution, LocalWorkerError> {
    let (t1_capabilities, t1_preparation, t1_executor) = t1.into_parts();
    let policies =
        enforcement_profiles(&t1_capabilities.definitions, &governance.executor_revision)?;
    let requirements = t1_capabilities
        .definitions
        .iter()
        .map(|definition| {
            (
                definition.name().to_owned(),
                definition.requirements().clone(),
            )
        })
        .collect();
    let mut definitions = base.capabilities.definitions;
    definitions.extend(t1_capabilities.definitions);
    definitions.sort_by(|left, right| left.name().cmp(right.name()));
    let f0 = base.f0;
    Ok(LocalGovernedExecution {
        capabilities: AgentToolCapabilities { definitions },
        authority: Box::new(AuthorityRouter {
            write: base.authority,
            t1: T1Authority {
                policy_revision: governance.policy_revision.clone(),
                workspace_capability_id: governance.workspace_capability_id.clone(),
                approved_digests: governance.approved_digests.clone(),
                denied_digests: governance.denied_digests.clone(),
            },
        }),
        executor: Box::new(ExecutorRouter {
            write: base.executor,
            t1: t1_executor,
        }),
        f0: garive_runtime::LocalF0Governance {
            preparation: Box::new(PreparationRouter {
                write: f0.preparation,
                t1: t1_preparation,
            }),
            recovery_content: f0.recovery_content,
            safety: Box::new(SafetyRouter {
                write: f0.safety,
                t1: T1Safety {
                    policy_revision: governance.policy_revision.clone(),
                    requirements,
                    approved_digests: governance.approved_digests,
                    denied_digests: governance.denied_digests,
                },
            }),
            sandbox: Box::new(SandboxRouter {
                write: f0.sandbox,
                t1: T1Sandbox {
                    policy_revision: governance.policy_revision.clone(),
                    workspace_capability_id: governance.workspace_capability_id,
                    profiles: policies,
                },
            }),
            context: garive_runtime::F0GovernanceContext {
                effective_policy_revision: governance.policy_revision,
                ..f0.context
            },
        },
    })
}

struct PreparationRouter {
    write: Box<dyn ToolPreparationPort>,
    t1: Box<dyn ToolPreparationPort>,
}

impl ToolPreparationPort for PreparationRouter {
    fn prepare(
        &self,
        intent: &ToolIntent,
    ) -> Result<PreparedToolCall, garive_tools::PreparationError> {
        if intent.tool_name() == WRITE_TOOL {
            self.write.prepare(intent)
        } else {
            self.t1.prepare(intent)
        }
    }
}

struct AuthorityRouter {
    write: Box<dyn AuthorityPort>,
    t1: T1Authority,
}

impl AuthorityPort for AuthorityRouter {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        if request.prepared.tool_name() == WRITE_TOOL {
            self.write.authorize(request)
        } else {
            self.t1.authorize(request)
        }
    }
}

struct T1Authority {
    policy_revision: String,
    workspace_capability_id: String,
    approved_digests: BTreeSet<String>,
    denied_digests: BTreeSet<String>,
}

impl AuthorityPort for T1Authority {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            let digest = request.prepared.input_digest();
            if self.denied_digests.contains(digest) {
                return Ok(AuthorityDecision::Deny {
                    safe_details: Some("user_denied".into()),
                });
            }
            if mutating(request.prepared.tool_name()) && !self.approved_digests.contains(digest) {
                return Ok(AuthorityDecision::InteractionRequired {
                    kind: InteractionKind::Approval,
                    prompt: approval_prompt(request.prepared.tool_name()),
                    response_schema: json!({"type":"boolean"}),
                    expiry_code: "turn_deadline".into(),
                });
            }
            Ok(AuthorityDecision::Approve {
                granted_requirements: request.prepared.requirements().clone(),
                constraints_digest: constraint_digest(
                    &self.policy_revision,
                    &self.workspace_capability_id,
                    digest,
                ),
                authority_revision: self.policy_revision.clone(),
            })
        })
    }
}

struct SafetyRouter {
    write: Box<dyn SafetyPort>,
    t1: T1Safety,
}

impl SafetyPort for SafetyRouter {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        if request.tool_name() == WRITE_TOOL {
            self.write.decide(request)
        } else {
            self.t1.decide(request)
        }
    }
}

struct T1Safety {
    policy_revision: String,
    requirements: BTreeMap<String, ExecutionRequirements>,
    approved_digests: BTreeSet<String>,
    denied_digests: BTreeSet<String>,
}

impl SafetyPort for T1Safety {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async move {
            let requirements = self
                .requirements
                .get(request.tool_name())
                .ok_or(GovernedRuntimePortError::InvalidBinding)?;
            let digest = request.prepared_digest();
            let disposition = if self.denied_digests.contains(digest) {
                SafetyDisposition::Deny
            } else if mutating(request.tool_name()) && !self.approved_digests.contains(digest) {
                SafetyDisposition::InteractionRequired
            } else {
                SafetyDisposition::Allow
            };
            let constraints = (disposition == SafetyDisposition::Allow).then(|| {
                constraint_digest(
                    &self.policy_revision,
                    "workspace",
                    request.prepared_digest(),
                )
            });
            Ok(SafetyEvaluation {
                decision: SafetyDecisionV1::new(
                    format!("workspace-t1-safety-{}", request.request_id()),
                    disposition,
                    request.invocation_id().clone(),
                    request.prepared_digest(),
                    constraints,
                    self.policy_revision.clone(),
                    match disposition {
                        SafetyDisposition::Allow => None,
                        SafetyDisposition::Deny => Some("safety_denied".into()),
                        SafetyDisposition::InteractionRequired => {
                            Some("safety_interaction_required".into())
                        }
                    },
                )
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
                granted_requirements: (disposition == SafetyDisposition::Allow)
                    .then(|| requirements.clone()),
                interaction: (disposition == SafetyDisposition::InteractionRequired).then(|| {
                    SafetyInteraction {
                        kind: InteractionKind::Approval,
                        prompt: approval_prompt(request.tool_name()),
                        response_schema: json!({"type":"boolean"}),
                        expiry_code: "turn_deadline".into(),
                    }
                }),
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

struct SandboxRouter {
    write: Box<dyn SandboxAdmissionPort>,
    t1: T1Sandbox,
}

impl SandboxAdmissionPort for SandboxRouter {
    fn admit(
        &mut self,
        request: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, GovernedRuntimePortError> {
        if request.safety_request.tool_name() == WRITE_TOOL {
            self.write.admit(request)
        } else {
            self.t1.admit(request)
        }
    }
}

struct T1Sandbox {
    policy_revision: String,
    workspace_capability_id: String,
    profiles: BTreeMap<String, EnforcementProfile>,
}

impl SandboxAdmissionPort for T1Sandbox {
    fn admit(
        &mut self,
        request: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, GovernedRuntimePortError> {
        let profile = self
            .profiles
            .get(request.safety_request.tool_name())
            .ok_or(GovernedRuntimePortError::InvalidBinding)?;
        let binding_id = stable_t1_id("binding", request.safety_request.invocation_id());
        let preflight_id = stable_t1_id("preflight", request.safety_request.invocation_id());
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                binding_id,
                self.workspace_capability_id.clone(),
                profile.executor_id,
                profile.executor_revision.clone(),
                self.policy_revision.clone(),
                profile.access.clone(),
                profile.sandbox.clone(),
            )
            .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            effective_limits_digest: profile
                .sandbox
                .digest()
                .map_err(|_| GovernedRuntimePortError::InvalidBinding)?,
            preflight_id,
            dispatch_attempt_id: t1_dispatch_attempt_id(
                profile.executor_id,
                request.safety_request.invocation_id(),
            )
            .ok_or(GovernedRuntimePortError::InvalidBinding)?,
        })
    }
}

fn stable_t1_id(kind: &str, invocation: &ToolInvocationId) -> String {
    format!(
        "workspace-t1-{kind}-{:x}",
        Sha256::digest(format!("{}:{kind}", invocation.as_str()).as_bytes())
    )
}

struct ExecutorRouter {
    write: Box<dyn ExecutorPort>,
    t1: Box<dyn ExecutorPort>,
}

impl ExecutorPort for ExecutorRouter {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &garive_tools::InvocationGrant,
    ) -> Result<garive_runtime::PreparedExecution, String> {
        if prepared.tool_name() == WRITE_TOOL {
            self.write.prepare(invocation_id, prepared, grant)
        } else {
            self.t1.prepare(invocation_id, prepared, grant)
        }
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        if command.execution.executor_id == "desktop.workspace.atomic-create" {
            self.write.dispatch(command)
        } else {
            self.t1.dispatch(command)
        }
    }

    fn acknowledge_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        if receipt.executor_id == "desktop.workspace.atomic-create" {
            self.write.acknowledge_receipt(invocation_id, receipt)
        } else {
            self.t1.acknowledge_receipt(invocation_id, receipt)
        }
    }

    fn reconcile_started_loss(
        &mut self,
        request: ExecutorRecoveryRequest<'_>,
    ) -> Result<(), ExecutorDispatchError> {
        if request.executor_id == "desktop.workspace.atomic-create" {
            self.write.reconcile_started_loss(request)
        } else {
            self.t1.reconcile_started_loss(request)
        }
    }
}

fn enforcement_profiles(
    definitions: &[ToolDefinition],
    executor_revision: &str,
) -> Result<BTreeMap<String, EnforcementProfile>, LocalWorkerError> {
    definitions
        .iter()
        .map(|definition| {
            let executor_id = match definition.name() {
                T1_READ_TEXT | T1_LIST | T1_SEARCH_TEXT => T1_WORKSPACE_EXECUTOR_ID,
                T1_APPLY_PATCH => T1_PATCH_EXECUTOR_ID,
                T1_PROCESS_RUN => T1_PROCESS_EXECUTOR_ID,
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
                    executor_revision: executor_revision.to_owned(),
                },
            ))
        })
        .collect()
}

fn mutating(tool_name: &str) -> bool {
    matches!(tool_name, T1_APPLY_PATCH | T1_PROCESS_RUN)
}

fn approval_prompt(tool_name: &str) -> serde_json::Value {
    json!({
        "schema_version":1,
        "title_key":"workspace.tool.approval.title",
        "message_text":format!("Allow {tool_name} in the attached Workspace?"),
        "action_label_key":"approval.allow_once",
        "cancel_label_key":"approval.deny"
    })
}

fn constraint_digest(policy_revision: &str, workspace: &str, prepared: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{policy_revision}:{workspace}:{prepared}").as_bytes())
    )
}
