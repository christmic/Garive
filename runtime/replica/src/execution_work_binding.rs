use garive_ledger::{DurableFact, ExecutionId, SessionId};
use serde_json::{json, Map, Value};

use crate::{F0GovernanceContext, PlanRuntimeError, SqliteLedger, SqliteLedgerError};

/// Exact Goal and Plan definition references durably owning one Execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionWorkBinding {
    goal_id: String,
    goal_revision: u64,
    goal_reference: String,
    plan_reference: String,
}

impl ExecutionWorkBinding {
    /// Returns the owning Goal identity proven by the Plan start transaction.
    pub fn goal_id(&self) -> &str {
        &self.goal_id
    }

    /// Returns the Goal revision anchored when the Plan was adopted.
    pub const fn goal_revision(&self) -> u64 {
        self.goal_revision
    }

    /// Returns canonical JSON binding the Goal identity, revision and digest.
    pub fn goal_reference(&self) -> &str {
        &self.goal_reference
    }

    /// Returns canonical JSON binding the Plan identity, revision and digest.
    pub fn plan_reference(&self) -> &str {
        &self.plan_reference
    }
}

/// Reconstructs the optional Plan-owned work binding from one fixed Session prefix.
pub fn reconstruct_execution_work_binding(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    execution_id: &ExecutionId,
) -> Result<Option<ExecutionWorkBinding>, PlanRuntimeError> {
    let watermark = ledger
        .session_watermark(session_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let facts = ledger
        .read_facts(session_id, 0, watermark.max_position, None)
        .map_err(map_ledger)?;
    let starts = matching_plan_starts(&facts, execution_id.as_str())?;
    let [] = starts.as_slice() else {
        let [start] = starts.as_slice() else {
            return Err(PlanRuntimeError::RecoveryCorrupt);
        };
        return reconstruct_bound(ledger, &facts, start, execution_id).map(Some);
    };
    Ok(None)
}

fn reconstruct_bound(
    ledger: &SqliteLedger,
    facts: &[DurableFact],
    start: &DurableFact,
    execution_id: &ExecutionId,
) -> Result<ExecutionWorkBinding, PlanRuntimeError> {
    let started = payload(start)?;
    let plan_id = text(&started, "plan_id")?;
    let plan_revision = unsigned(&started, "plan_revision")?;
    let start_version = ledger
        .fact_commit_version(&start.fact_id)
        .map_err(map_ledger)?
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    let execution_facts = facts
        .iter()
        .filter(|fact| {
            fact.kind.as_str() == "execution.started"
                && fact.execution_id.as_ref() == Some(execution_id)
        })
        .collect::<Vec<_>>();
    let [execution] = execution_facts.as_slice() else {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    };
    if ledger
        .fact_commit_version(&execution.fact_id)
        .map_err(map_ledger)?
        != Some(start_version)
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let execution_payload = payload(execution)?;
    let proposals = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.proposed" && fact.position < start.position)
        .filter_map(|fact| {
            let value = payload(fact).ok()?;
            (value.get("plan_id")?.as_str()? == plan_id
                && value.get("plan_revision")?.as_u64()? == plan_revision)
                .then_some(value)
        })
        .collect::<Vec<_>>();
    let [proposal] = proposals.as_slice() else {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    };
    let snapshot_digest = text(proposal, "agent_snapshot_digest")?;
    if text(&started, "execution_snapshot_digest")? != snapshot_digest
        || text(&execution_payload, "snapshot_digest")? != snapshot_digest
    {
        return Err(PlanRuntimeError::RecoveryCorrupt);
    }
    let goal_id = text(proposal, "goal_id")?;
    let goal_revision = unsigned(proposal, "goal_revision")?;
    let goal_digest = text(proposal, "goal_definition_digest")?;
    let goal_facts = facts
        .iter()
        .filter(|fact| fact.position <= start.position)
        .cloned()
        .collect::<Vec<_>>();
    let goals = crate::goal_recovery::reconstruct_goal_graph_from_facts(
        &goal_facts,
        start_version,
        start.position,
    )
    .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
    let goal = goals
        .get(goal_id)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)?;
    if goal.snapshot.state().is_terminal()
        || goal.snapshot.revision() < goal_revision
        || goal
            .snapshot
            .definition()
            .digest()
            .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?
            != goal_digest
    {
        return Err(PlanRuntimeError::BindingStale);
    }
    let goal_reference = serde_jcs::to_string(&json!({
        "definition_digest":goal_digest,
        "goal_id":goal_id,
        "revision":goal_revision,
    }))
    .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
    let plan_reference = serde_jcs::to_string(&json!({
        "definition_digest":text(proposal,"plan_digest")?,
        "plan_id":plan_id,
        "revision":plan_revision,
    }))
    .map_err(|_| PlanRuntimeError::RecoveryCorrupt)?;
    Ok(ExecutionWorkBinding {
        goal_id: goal_id.into(),
        goal_revision,
        goal_reference,
        plan_reference,
    })
}

fn matching_plan_starts<'a>(
    facts: &'a [DurableFact],
    execution_id: &str,
) -> Result<Vec<&'a DurableFact>, PlanRuntimeError> {
    let mut starts = Vec::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.step.started")
    {
        let value = payload(fact)?;
        if value.get("execution_id").and_then(Value::as_str) == Some(execution_id) {
            starts.push(fact);
        }
    }
    Ok(starts)
}

pub(crate) fn governance_matches(
    binding: Option<&ExecutionWorkBinding>,
    context: &F0GovernanceContext,
) -> bool {
    match binding {
        Some(binding) => {
            context.goal_reference.as_deref() == Some(binding.goal_reference())
                && context.plan_reference.as_deref() == Some(binding.plan_reference())
        }
        None => context.goal_reference.is_none() && context.plan_reference.is_none(),
    }
}

pub(crate) fn bind_governance_context(
    binding: Option<&ExecutionWorkBinding>,
    context: &mut F0GovernanceContext,
) -> bool {
    if context.goal_reference.is_some() || context.plan_reference.is_some() {
        return false;
    }
    if let Some(binding) = binding {
        context.goal_reference = Some(binding.goal_reference().into());
        context.plan_reference = Some(binding.plan_reference().into());
    }
    true
}

fn payload(fact: &DurableFact) -> Result<Map<String, Value>, PlanRuntimeError> {
    serde_json::from_str::<Value>(fact.payload.as_json())
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, PlanRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, PlanRuntimeError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or(PlanRuntimeError::RecoveryCorrupt)
}

fn map_ledger(error: SqliteLedgerError) -> PlanRuntimeError {
    match error {
        SqliteLedgerError::Storage(_) => PlanRuntimeError::DurabilityFailure,
        _ => PlanRuntimeError::RecoveryCorrupt,
    }
}
