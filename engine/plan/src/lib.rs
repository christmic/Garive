//! Portable Goal-bound Plan definitions, topology, and pure progress reduction.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeSet;

use serde::Serialize;
use sha2::{Digest, Sha256};

mod lifecycle;
mod topology;

pub use lifecycle::{PlanSnapshot, PlanState, PlanTransition, StepProgress, StepState};

const DEFINITION_CONTRACT: &str = "garive.plan-definition";
const CONTRACT_VERSION: u8 = 1;

macro_rules! identity {
    ($name:ident, $label:literal) => {
        #[doc = concat!("Non-empty opaque ", $label, " identity.")]
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[doc = concat!("Validates and constructs a ", $label, " identity.")]
            pub fn new(value: impl Into<String>) -> Result<Self, PlanError> {
                let value = value.into();
                if value.is_empty() {
                    Err(PlanError::new(PlanErrorCode::PlanInvalid))
                } else {
                    Ok(Self(value))
                }
            }

            /// Returns the opaque identity text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

identity!(PlanId, "Plan");
identity!(PlanStepId, "Plan step");

/// Stable portable Plan failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanErrorCode {
    /// Identity, binding, bounds, step, or digest is malformed.
    PlanInvalid,
    /// The declared dependency graph contains a cycle.
    PlanCycle,
    /// A requested progress transition is not admitted.
    PlanTransitionInvalid,
    /// A step is not currently ready to claim.
    StepNotReady,
    /// A hard Plan or step bound has been exhausted.
    PlanBoundExceeded,
}

/// Typed portable Plan failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanError {
    code: PlanErrorCode,
}

impl PlanError {
    const fn new(code: PlanErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure classification.
    pub const fn code(self) -> PlanErrorCode {
        self.code
    }
}

/// Exact capability revision required by one step.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PlanCapabilityReference {
    name: String,
    exact_revision: String,
}

impl PlanCapabilityReference {
    /// Validates one exact non-empty capability reference.
    pub fn new(
        name: impl Into<String>,
        exact_revision: impl Into<String>,
    ) -> Result<Self, PlanError> {
        let value = Self {
            name: name.into(),
            exact_revision: exact_revision.into(),
        };
        if value.name.is_empty() || value.exact_revision.is_empty() {
            Err(PlanError::new(PlanErrorCode::PlanInvalid))
        } else {
            Ok(value)
        }
    }
}

/// Explicit non-zero hard bounds for one Plan revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanBoundsV1 {
    max_steps: u32,
    max_parallel_ready: u32,
    max_total_attempts: u32,
    token_budget: Option<u64>,
    duration_budget_ms: Option<u64>,
}

impl PlanBoundsV1 {
    /// Validates mandatory and optional bounds as non-zero.
    pub fn new(
        max_steps: u32,
        max_parallel_ready: u32,
        max_total_attempts: u32,
        token_budget: Option<u64>,
        duration_budget_ms: Option<u64>,
    ) -> Result<Self, PlanError> {
        if max_steps == 0
            || max_parallel_ready == 0
            || max_parallel_ready > max_steps
            || max_total_attempts == 0
            || token_budget == Some(0)
            || duration_budget_ms == Some(0)
        {
            return Err(PlanError::new(PlanErrorCode::PlanInvalid));
        }
        Ok(Self {
            max_steps,
            max_parallel_ready,
            max_total_attempts,
            token_budget,
            duration_budget_ms,
        })
    }

    /// Returns the maximum admitted step count.
    pub const fn max_steps(&self) -> u32 {
        self.max_steps
    }

    /// Returns the maximum simultaneous claimed/running count.
    pub const fn max_parallel_ready(&self) -> u32 {
        self.max_parallel_ready
    }

    /// Returns the maximum attempts across the revision.
    pub const fn max_total_attempts(&self) -> u32 {
        self.max_total_attempts
    }
}

/// One immutable executable node in declaration/tie-break order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanStepV1 {
    step_id: PlanStepId,
    objective: String,
    depends_on: BTreeSet<PlanStepId>,
    completion_criteria: BTreeSet<String>,
    required_capabilities: BTreeSet<PlanCapabilityReference>,
    input_bindings: BTreeSet<String>,
    max_attempts: u32,
}

impl PlanStepV1 {
    /// Validates one step and canonicalizes every set-valued binding.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        step_id: PlanStepId,
        objective: impl Into<String>,
        depends_on: impl IntoIterator<Item = PlanStepId>,
        completion_criteria: impl IntoIterator<Item = String>,
        required_capabilities: impl IntoIterator<Item = PlanCapabilityReference>,
        input_bindings: impl IntoIterator<Item = String>,
        max_attempts: u32,
    ) -> Result<Self, PlanError> {
        let objective = objective.into();
        let depends_on = unique(depends_on)?;
        let completion_criteria = unique_non_empty(completion_criteria)?;
        let required_capabilities = unique(required_capabilities)?;
        let input_bindings = unique_digests(input_bindings)?;
        if objective.is_empty()
            || completion_criteria.is_empty()
            || max_attempts == 0
            || depends_on.contains(&step_id)
        {
            return Err(PlanError::new(PlanErrorCode::PlanInvalid));
        }
        Ok(Self {
            step_id,
            objective,
            depends_on,
            completion_criteria,
            required_capabilities,
            input_bindings,
            max_attempts,
        })
    }

    /// Returns the stable step identity.
    pub const fn step_id(&self) -> &PlanStepId {
        &self.step_id
    }

    /// Returns canonical direct dependencies.
    pub const fn depends_on(&self) -> &BTreeSet<PlanStepId> {
        &self.depends_on
    }

    /// Returns criterion identities covered by this step.
    pub const fn completion_criteria(&self) -> &BTreeSet<String> {
        &self.completion_criteria
    }

    /// Returns the hard attempt limit for this step.
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

/// Immutable canonical Plan revision content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlanDefinitionV1 {
    contract: &'static str,
    version: u8,
    plan_id: PlanId,
    plan_revision: u64,
    goal_id: String,
    goal_revision: u64,
    goal_definition_digest: String,
    agent_snapshot_digest: String,
    tool_catalogue_digest: String,
    safety_policy_revision: String,
    steps: Vec<PlanStepV1>,
    bounds: PlanBoundsV1,
}

impl PlanDefinitionV1 {
    /// Validates all frozen bindings, coverage, declared order, and DAG topology.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plan_id: PlanId,
        plan_revision: u64,
        goal_id: impl Into<String>,
        goal_revision: u64,
        goal_definition_digest: impl Into<String>,
        agent_snapshot_digest: impl Into<String>,
        tool_catalogue_digest: impl Into<String>,
        safety_policy_revision: impl Into<String>,
        steps: Vec<PlanStepV1>,
        bounds: PlanBoundsV1,
        required_goal_criteria: &BTreeSet<String>,
        already_satisfied_criteria: &BTreeSet<String>,
        available_capabilities: &BTreeSet<PlanCapabilityReference>,
    ) -> Result<Self, PlanError> {
        let value = Self {
            contract: DEFINITION_CONTRACT,
            version: CONTRACT_VERSION,
            plan_id,
            plan_revision,
            goal_id: goal_id.into(),
            goal_revision,
            goal_definition_digest: goal_definition_digest.into(),
            agent_snapshot_digest: agent_snapshot_digest.into(),
            tool_catalogue_digest: tool_catalogue_digest.into(),
            safety_policy_revision: safety_policy_revision.into(),
            steps,
            bounds,
        };
        value.validate(
            required_goal_criteria,
            already_satisfied_criteria,
            available_capabilities,
        )?;
        Ok(value)
    }

    fn validate(
        &self,
        required: &BTreeSet<String>,
        satisfied: &BTreeSet<String>,
        available_capabilities: &BTreeSet<PlanCapabilityReference>,
    ) -> Result<(), PlanError> {
        let ids: BTreeSet<_> = self
            .steps
            .iter()
            .map(|step| step.step_id().clone())
            .collect();
        let covered: BTreeSet<_> = self
            .steps
            .iter()
            .flat_map(|step| step.completion_criteria.iter().cloned())
            .chain(satisfied.iter().cloned())
            .collect();
        if self.plan_revision == 0
            || self.goal_id.is_empty()
            || self.goal_revision == 0
            || !valid_digest(&self.goal_definition_digest)
            || !valid_digest(&self.agent_snapshot_digest)
            || !valid_digest(&self.tool_catalogue_digest)
            || self.safety_policy_revision.is_empty()
            || self.steps.is_empty()
            || self.steps.len() > self.bounds.max_steps as usize
            || ids.len() != self.steps.len()
            || self
                .steps
                .iter()
                .any(|step| !step.depends_on.is_subset(&ids))
            || self
                .steps
                .iter()
                .any(|step| !step.completion_criteria.is_subset(required))
            || self
                .steps
                .iter()
                .any(|step| !step.required_capabilities.is_subset(available_capabilities))
            || required.is_empty()
            || !satisfied.is_subset(required)
            || !required.is_subset(&covered)
        {
            return Err(PlanError::new(PlanErrorCode::PlanInvalid));
        }
        topology::validate_acyclic(&self.steps)
    }

    /// Returns steps in semantic declaration/tie-break order.
    pub fn steps(&self) -> &[PlanStepV1] {
        &self.steps
    }

    /// Returns immutable revision bounds.
    pub const fn bounds(&self) -> &PlanBoundsV1 {
        &self.bounds
    }

    /// Returns lowercase SHA-256 over the RFC 8785 definition.
    pub fn digest(&self) -> Result<String, PlanError> {
        Ok(format!(
            "{:x}",
            Sha256::digest(self.canonical_json()?.as_bytes())
        ))
    }

    /// Returns the exact RFC 8785 Plan definition document.
    pub fn canonical_json(&self) -> Result<String, PlanError> {
        serde_jcs::to_string(self).map_err(|_| PlanError::new(PlanErrorCode::PlanInvalid))
    }

    /// Binds one step to all cross-revision inputs that affect safe carry-forward.
    pub fn step_digest(&self, step_id: &PlanStepId) -> Result<String, PlanError> {
        let step = self
            .steps
            .iter()
            .find(|step| step.step_id() == step_id)
            .ok_or_else(|| PlanError::new(PlanErrorCode::PlanInvalid))?;
        let document = StepDigestDocument {
            contract: "garive.plan-step",
            version: 1,
            plan_id: &self.plan_id,
            goal_id: &self.goal_id,
            goal_revision: self.goal_revision,
            goal_definition_digest: &self.goal_definition_digest,
            agent_snapshot_digest: &self.agent_snapshot_digest,
            tool_catalogue_digest: &self.tool_catalogue_digest,
            safety_policy_revision: &self.safety_policy_revision,
            step,
        };
        let bytes =
            serde_jcs::to_vec(&document).map_err(|_| PlanError::new(PlanErrorCode::PlanInvalid))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Serialize)]
struct StepDigestDocument<'a> {
    contract: &'static str,
    version: u8,
    plan_id: &'a PlanId,
    goal_id: &'a str,
    goal_revision: u64,
    goal_definition_digest: &'a str,
    agent_snapshot_digest: &'a str,
    tool_catalogue_digest: &'a str,
    safety_policy_revision: &'a str,
    step: &'a PlanStepV1,
}

fn unique<T: Ord>(values: impl IntoIterator<Item = T>) -> Result<BTreeSet<T>, PlanError> {
    let values: Vec<_> = values.into_iter().collect();
    let count = values.len();
    let unique: BTreeSet<_> = values.into_iter().collect();
    if unique.len() == count {
        Ok(unique)
    } else {
        Err(PlanError::new(PlanErrorCode::PlanInvalid))
    }
}

fn unique_non_empty(
    values: impl IntoIterator<Item = String>,
) -> Result<BTreeSet<String>, PlanError> {
    let values: Vec<_> = values.into_iter().collect();
    let unique: BTreeSet<_> = values.iter().cloned().collect();
    if values.iter().any(String::is_empty) || values.len() != unique.len() {
        Err(PlanError::new(PlanErrorCode::PlanInvalid))
    } else {
        Ok(unique)
    }
}

fn unique_digests(values: impl IntoIterator<Item = String>) -> Result<BTreeSet<String>, PlanError> {
    let values: Vec<_> = values.into_iter().collect();
    let unique: BTreeSet<_> = values.iter().cloned().collect();
    if values.iter().any(|value| !valid_digest(value)) || values.len() != unique.len() {
        Err(PlanError::new(PlanErrorCode::PlanInvalid))
    } else {
        Ok(unique)
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
