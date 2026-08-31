//! Runtime-owned F0 safety decision and concrete sandbox preflight binding.

use garive_tools::{
    InvocationGrant, PreparedToolCall, SandboxRequirementsV1, ToolAccessPolicyV1, ToolInvocationId,
};
use sha2::{Digest, Sha256};

use crate::PreparedExecution;

/// Exact Runtime-owned input evaluated by F0 safety policy before C5 grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafetyRequestV1 {
    request_id: String,
    invocation_id: ToolInvocationId,
    prepared_digest: String,
    tool_name: String,
    tool_revision: String,
    actor_authority_reference: String,
    goal_reference: Option<String>,
    plan_reference: Option<String>,
    exact_access_digest: String,
    sandbox_requirements_digest: String,
    effective_policy_revision: String,
}

impl SafetyRequestV1 {
    /// Derives all tool and access bindings from one exact Prepared-v3 call.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: impl Into<String>,
        invocation_id: ToolInvocationId,
        prepared: &PreparedToolCall,
        actor_authority_reference: impl Into<String>,
        goal_reference: Option<String>,
        plan_reference: Option<String>,
        effective_policy_revision: impl Into<String>,
    ) -> Result<Self, SandboxPreflightError> {
        let accesses = prepared
            .invocation_accesses()
            .ok_or(SandboxPreflightError::InvalidBinding)?;
        let exact_access_digest = serde_jcs::to_vec(accesses)
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
            .map_err(|_| SandboxPreflightError::InvalidBinding)?;
        let value = Self {
            request_id: request_id.into(),
            invocation_id,
            prepared_digest: prepared.input_digest().into(),
            tool_name: prepared.tool_name().into(),
            tool_revision: prepared.tool_revision().into(),
            actor_authority_reference: actor_authority_reference.into(),
            goal_reference,
            plan_reference,
            exact_access_digest,
            sandbox_requirements_digest: prepared
                .sandbox_requirements_digest()
                .ok_or(SandboxPreflightError::InvalidBinding)?
                .into(),
            effective_policy_revision: effective_policy_revision.into(),
        };
        if prepared.contract_version() != 3
            || [
                value.request_id.as_str(),
                value.actor_authority_reference.as_str(),
                value.effective_policy_revision.as_str(),
            ]
            .iter()
            .any(|field| field.is_empty())
            || value.goal_reference.as_deref() == Some("")
            || value.plan_reference.as_deref() == Some("")
        {
            return Err(SandboxPreflightError::InvalidBinding);
        }
        Ok(value)
    }

    /// Returns the stable request identity.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Returns the exact invocation identity.
    pub const fn invocation_id(&self) -> &ToolInvocationId {
        &self.invocation_id
    }

    /// Returns the Prepared Call digest evaluated by policy.
    pub fn prepared_digest(&self) -> &str {
        &self.prepared_digest
    }

    /// Returns the effective policy revision used by the decision.
    pub fn effective_policy_revision(&self) -> &str {
        &self.effective_policy_revision
    }

    /// Returns the exact provider-neutral Tool identity under evaluation.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// Returns the exact immutable Tool revision under evaluation.
    pub fn tool_revision(&self) -> &str {
        &self.tool_revision
    }

    pub(crate) fn actor_authority_reference(&self) -> &str {
        &self.actor_authority_reference
    }

    pub(crate) fn goal_reference(&self) -> Option<&str> {
        self.goal_reference.as_deref()
    }

    pub(crate) fn plan_reference(&self) -> Option<&str> {
        self.plan_reference.as_deref()
    }

    pub(crate) fn exact_access_digest(&self) -> &str {
        &self.exact_access_digest
    }

    pub(crate) fn sandbox_requirements_digest(&self) -> &str {
        &self.sandbox_requirements_digest
    }
}

/// Closed policy outcome before a C5 grant may reach sandbox preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyDisposition {
    /// Exact request is permitted under narrower constraints.
    Allow,
    /// Exact request is rejected.
    Deny,
    /// Existing C5 interaction must resolve before reevaluation.
    InteractionRequired,
}

/// Validated Runtime safety decision bound to one exact invocation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafetyDecisionV1 {
    decision_id: String,
    disposition: SafetyDisposition,
    invocation_id: ToolInvocationId,
    prepared_digest: String,
    constraints_digest: Option<String>,
    policy_revision: String,
    safe_code: Option<String>,
}

impl SafetyDecisionV1 {
    /// Constructs a decision with disposition-specific constraint/code fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        decision_id: impl Into<String>,
        disposition: SafetyDisposition,
        invocation_id: ToolInvocationId,
        prepared_digest: impl Into<String>,
        constraints_digest: Option<String>,
        policy_revision: impl Into<String>,
        safe_code: Option<String>,
    ) -> Result<Self, SandboxPreflightError> {
        let value = Self {
            decision_id: decision_id.into(),
            disposition,
            invocation_id,
            prepared_digest: prepared_digest.into(),
            constraints_digest,
            policy_revision: policy_revision.into(),
            safe_code,
        };
        let fields_match = match value.disposition {
            SafetyDisposition::Allow => {
                non_empty(value.constraints_digest.as_deref()) && value.safe_code.is_none()
            }
            SafetyDisposition::Deny => {
                value.constraints_digest.is_none()
                    && value.safe_code.as_deref() == Some("safety_denied")
            }
            SafetyDisposition::InteractionRequired => {
                value.constraints_digest.is_none()
                    && value.safe_code.as_deref() == Some("safety_interaction_required")
            }
        };
        if value.decision_id.is_empty()
            || value.prepared_digest.is_empty()
            || value.policy_revision.is_empty()
            || !fields_match
        {
            return Err(SandboxPreflightError::InvalidBinding);
        }
        Ok(value)
    }

    /// Returns the stable policy outcome.
    pub const fn disposition(&self) -> SafetyDisposition {
        self.disposition
    }

    /// Returns the Runtime-owned decision identity.
    pub fn decision_id(&self) -> &str {
        &self.decision_id
    }

    pub(crate) const fn invocation_id(&self) -> &ToolInvocationId {
        &self.invocation_id
    }

    pub(crate) fn prepared_digest(&self) -> &str {
        &self.prepared_digest
    }

    pub(crate) fn constraints_digest(&self) -> Option<&str> {
        self.constraints_digest.as_deref()
    }

    pub(crate) fn policy_revision(&self) -> &str {
        &self.policy_revision
    }

    pub(crate) fn safe_code(&self) -> Option<&str> {
        self.safe_code.as_deref()
    }
}

/// Immutable selected executor, workspace, scope and enforcement proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxBindingV1 {
    binding_id: String,
    workspace_capability_id: String,
    executor_id: String,
    executor_revision: String,
    policy_revision: String,
    access_scope: ToolAccessPolicyV1,
    enforcement: SandboxRequirementsV1,
}

impl SandboxBindingV1 {
    /// Validates non-empty Runtime identities and freezes the complete binding.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        binding_id: impl Into<String>,
        workspace_capability_id: impl Into<String>,
        executor_id: impl Into<String>,
        executor_revision: impl Into<String>,
        policy_revision: impl Into<String>,
        access_scope: ToolAccessPolicyV1,
        enforcement: SandboxRequirementsV1,
    ) -> Result<Self, SandboxPreflightError> {
        let value = Self {
            binding_id: binding_id.into(),
            workspace_capability_id: workspace_capability_id.into(),
            executor_id: executor_id.into(),
            executor_revision: executor_revision.into(),
            policy_revision: policy_revision.into(),
            access_scope,
            enforcement,
        };
        if [
            &value.binding_id,
            &value.workspace_capability_id,
            &value.executor_id,
            &value.executor_revision,
            &value.policy_revision,
        ]
        .iter()
        .any(|field| field.is_empty())
        {
            return Err(SandboxPreflightError::InvalidBinding);
        }
        Ok(value)
    }

    /// Returns the opaque workspace capability reference.
    pub fn workspace_capability_id(&self) -> &str {
        &self.workspace_capability_id
    }

    pub(crate) fn binding_id(&self) -> &str {
        &self.binding_id
    }

    pub(crate) fn executor_id(&self) -> &str {
        &self.executor_id
    }

    pub(crate) fn executor_revision(&self) -> &str {
        &self.executor_revision
    }

    pub(crate) fn policy_revision(&self) -> &str {
        &self.policy_revision
    }

    pub(crate) const fn access_scope(&self) -> &ToolAccessPolicyV1 {
        &self.access_scope
    }

    pub(crate) const fn enforcement(&self) -> &SandboxRequirementsV1 {
        &self.enforcement
    }
}

/// Stable pre-start F0 failure; no variant permits dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxPreflightError {
    /// Identity, digest, grant or structural binding is invalid.
    InvalidBinding,
    /// Safety policy did not allow this exact request.
    DecisionNotAllowed,
    /// Executor cannot prove every requested control and limit.
    EnforcementUnsupported,
    /// Exact invocation resource is outside the selected workspace scope.
    ScopeMismatch,
    /// Policy revision changed before start.
    BindingStale,
}

/// Verifies F0/C5 bindings and returns an executor selection without dispatch.
pub fn preflight_sandbox(
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
    decision: &SafetyDecisionV1,
    binding: &SandboxBindingV1,
    dispatch_attempt_id: impl Into<String>,
) -> Result<PreparedExecution, SandboxPreflightError> {
    let dispatch_attempt_id = dispatch_attempt_id.into();
    if decision.disposition != SafetyDisposition::Allow {
        return Err(SandboxPreflightError::DecisionNotAllowed);
    }
    if prepared.contract_version() != 3
        || decision.invocation_id != *invocation_id
        || decision.prepared_digest != prepared.input_digest()
        || grant.invocation_id != *invocation_id
        || grant.prepared_digest != prepared.input_digest()
        || grant.tool_name != prepared.tool_name()
        || grant.tool_revision != prepared.tool_revision()
        || decision.constraints_digest.as_deref() != Some(grant.constraints_digest.as_str())
        || dispatch_attempt_id.is_empty()
    {
        return Err(SandboxPreflightError::InvalidBinding);
    }
    if decision.policy_revision != grant.authority_revision
        || binding.policy_revision != decision.policy_revision
    {
        return Err(SandboxPreflightError::BindingStale);
    }
    let requested = prepared
        .sandbox_requirements()
        .ok_or(SandboxPreflightError::InvalidBinding)?;
    if !requested.is_covered_by(&binding.enforcement) {
        return Err(SandboxPreflightError::EnforcementUnsupported);
    }
    let accesses = prepared
        .invocation_accesses()
        .ok_or(SandboxPreflightError::InvalidBinding)?;
    if !binding.access_scope.covers(accesses) {
        return Err(SandboxPreflightError::ScopeMismatch);
    }
    Ok(PreparedExecution {
        executor_id: binding.executor_id.clone(),
        executor_revision: binding.executor_revision.clone(),
        dispatch_attempt_id,
    })
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}
