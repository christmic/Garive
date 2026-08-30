use std::collections::BTreeSet;

use serde::Deserialize;

use crate::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanError, PlanErrorCode, PlanId,
    PlanStepId, PlanStepV1, CONTRACT_VERSION, DEFINITION_CONTRACT,
};

impl PlanDefinitionV1 {
    /// Reconstructs and revalidates one exact canonical definition document.
    pub fn from_canonical_json(
        json: &str,
        required_goal_criteria: &BTreeSet<String>,
        already_satisfied_criteria: &BTreeSet<String>,
        available_capabilities: &BTreeSet<PlanCapabilityReference>,
    ) -> Result<Self, PlanError> {
        let raw: RawPlanDefinition =
            serde_json::from_str(json).map_err(|_| PlanError::new(PlanErrorCode::PlanInvalid))?;
        if raw.contract != DEFINITION_CONTRACT || raw.version != CONTRACT_VERSION {
            return Err(PlanError::new(PlanErrorCode::PlanInvalid));
        }
        let bounds = PlanBoundsV1::new(
            raw.bounds.max_steps,
            raw.bounds.max_parallel_ready,
            raw.bounds.max_total_attempts,
            raw.bounds.token_budget,
            raw.bounds.duration_budget_ms,
        )?;
        let steps = raw
            .steps
            .into_iter()
            .map(RawPlanStep::build)
            .collect::<Result<Vec<_>, _>>()?;
        let value = Self::new(
            PlanId::new(raw.plan_id)?,
            raw.plan_revision,
            raw.goal_id,
            raw.goal_revision,
            raw.goal_definition_digest,
            raw.agent_snapshot_digest,
            raw.tool_catalogue_digest,
            raw.safety_policy_revision,
            steps,
            bounds,
            required_goal_criteria,
            already_satisfied_criteria,
            available_capabilities,
        )?;
        if value.canonical_json()? != json {
            return Err(PlanError::new(PlanErrorCode::PlanInvalid));
        }
        Ok(value)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlanDefinition {
    contract: String,
    version: u8,
    plan_id: String,
    plan_revision: u64,
    goal_id: String,
    goal_revision: u64,
    goal_definition_digest: String,
    agent_snapshot_digest: String,
    tool_catalogue_digest: String,
    safety_policy_revision: String,
    steps: Vec<RawPlanStep>,
    bounds: RawPlanBounds,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlanBounds {
    max_steps: u32,
    max_parallel_ready: u32,
    max_total_attempts: u32,
    token_budget: Option<u64>,
    duration_budget_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlanStep {
    step_id: String,
    objective: String,
    depends_on: Vec<String>,
    completion_criteria: Vec<String>,
    required_capabilities: Vec<RawCapability>,
    input_bindings: Vec<String>,
    max_attempts: u32,
}

impl RawPlanStep {
    fn build(self) -> Result<PlanStepV1, PlanError> {
        PlanStepV1::new(
            PlanStepId::new(self.step_id)?,
            self.objective,
            self.depends_on
                .into_iter()
                .map(PlanStepId::new)
                .collect::<Result<Vec<_>, _>>()?,
            self.completion_criteria,
            self.required_capabilities
                .into_iter()
                .map(RawCapability::build)
                .collect::<Result<Vec<_>, _>>()?,
            self.input_bindings,
            self.max_attempts,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCapability {
    name: String,
    exact_revision: String,
}

impl RawCapability {
    fn build(self) -> Result<PlanCapabilityReference, PlanError> {
        PlanCapabilityReference::new(self.name, self.exact_revision)
    }
}
