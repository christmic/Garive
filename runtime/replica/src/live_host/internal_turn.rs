use std::collections::BTreeSet;

use garive_ledger::{DurableFact, ExecutionId, TurnId};
use serde::Deserialize;

use super::LiveHostError;

#[derive(Default)]
pub(super) struct InternalPlannerTurns {
    turn_ids: BTreeSet<String>,
    execution_ids: BTreeSet<String>,
}

impl InternalPlannerTurns {
    pub(super) fn from_facts(facts: &[DurableFact]) -> Result<Self, LiveHostError> {
        let mut turns = Self::default();
        for fact in facts.iter().filter(|fact| {
            matches!(
                fact.kind.as_str(),
                "plan.proposal.requested" | "plan.replan.proposal.requested"
            )
        }) {
            fact.verify().map_err(|_| LiveHostError::CorruptState)?;
            if fact.schema_version != 1
                || fact.turn_id.is_some()
                || fact.execution_id.is_some()
                || fact.model_request_id.is_some()
                || fact.tool_invocation_id.is_some()
            {
                return Err(LiveHostError::CorruptState);
            }
            let request: PlannerRequest = serde_json::from_str(fact.payload.as_json())
                .map_err(|_| LiveHostError::CorruptState)?;
            TurnId::try_from(request.turn_id.as_str()).map_err(|_| LiveHostError::CorruptState)?;
            ExecutionId::try_from(request.execution_id.as_str())
                .map_err(|_| LiveHostError::CorruptState)?;
            if !turns.turn_ids.insert(request.turn_id)
                || !turns.execution_ids.insert(request.execution_id)
            {
                return Err(LiveHostError::CorruptState);
            }
        }
        Ok(turns)
    }

    pub(super) fn contains_fact(&self, fact: &DurableFact) -> bool {
        fact.turn_id
            .as_ref()
            .is_some_and(|turn| self.turn_ids.contains(turn.as_str()))
            || fact
                .execution_id
                .as_ref()
                .is_some_and(|execution| self.execution_ids.contains(execution.as_str()))
    }

    pub(super) fn remove_activities<T>(
        &self,
        activities: &mut std::collections::BTreeMap<String, T>,
    ) {
        activities.retain(|turn_id, _| !self.turn_ids.contains(turn_id));
    }
}

#[derive(Deserialize)]
struct PlannerRequest {
    turn_id: String,
    execution_id: String,
}
