//! Fixed-prefix construction of one initial executable Plan proposal.

use std::{collections::BTreeSet, future::Future, path::Path, pin::Pin, sync::Arc};

use garive_goal::GoalState;
use garive_ledger::SessionId;
use garive_plan::{PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanStepV1};
use sha2::{Digest, Sha256};

use crate::{
    commit_plan_command, plan_propose_plan, reconstruct_goal, reconstruct_plan_graph,
    PlanCommandContext, PlanRuntimeError, RuntimeAgentCatalogue, SqliteLedger,
};

/// Read-only Goal content and ceilings exposed to a configured planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanProposalRequest {
    /// Exact Goal identity.
    pub goal_id: String,
    /// Exact Goal lifecycle revision.
    pub goal_revision: u64,
    /// Canonical immutable Goal definition digest.
    pub goal_definition_digest: String,
    /// Bounded user/product objective.
    pub objective: String,
    /// Criterion identities in semantic declaration order.
    pub criterion_ids: Vec<String>,
    /// Exact capabilities admitted by the Goal.
    pub available_capabilities: BTreeSet<PlanCapabilityReference>,
    /// Maximum total attempts admitted by the Goal.
    pub max_total_attempts: u32,
}

/// Untrusted topology content returned by a configured planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanProposalContent {
    /// Steps in semantic declaration and tie-break order.
    pub steps: Vec<PlanStepV1>,
    /// Requested Plan-local bounds.
    pub bounds: PlanBoundsV1,
}

/// Asynchronous provider-neutral proposal future.
pub type PlanProposalFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PlanProposalContent, PlanProposalPortError>> + Send + 'a>>;

/// Configured planner boundary with no Ledger or execution-binding authority.
pub trait PlanProposalPort: Send + Sync {
    /// Stable secret-free planner revision recorded as proposal authority.
    fn proposer_reference(&self) -> &str;

    /// Produces topology only; Runtime supplies every identity and frozen binding.
    fn propose<'a>(&'a self, request: &'a PlanProposalRequest) -> PlanProposalFuture<'a>;
}

/// Stable failures returned by a configured planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanProposalPortError {
    /// The configured planner is temporarily unavailable.
    Unavailable,
    /// The planner could not produce bounded valid topology.
    InvalidOutput,
}

/// Durable receipt for one fixed-prefix initial proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposedGoalPlan {
    /// Runtime-derived deterministic Plan identity.
    pub plan_id: String,
    /// Initial immutable revision, always one.
    pub plan_revision: u64,
    /// Canonical definition digest committed in `plan.proposed`.
    pub plan_definition_digest: String,
}

/// Stable secret-free proposal construction failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanProposalRuntimeError {
    /// Runtime-owned metadata or planner identity is malformed.
    InvalidInput,
    /// Goal, Session installation or complete Plan graph is corrupt or stale.
    CorruptState,
    /// This Goal already owns Plan lineage and needs replan semantics instead.
    ExistingPlanLineage,
    /// The configured planner failed or returned inadmissible topology.
    ProposalFailed,
    /// Portable Plan planning or optimistic durability failed.
    Plan(PlanRuntimeError),
}

/// Constructs and commits one initial Plan without exposing bindings to the planner.
pub async fn propose_initial_goal_plan_once(
    database_path: &Path,
    session_id: &SessionId,
    goal_id: &str,
    recorded_at: &str,
    catalogue: Arc<RuntimeAgentCatalogue>,
    port: &dyn PlanProposalPort,
) -> Result<ProposedGoalPlan, PlanProposalRuntimeError> {
    if goal_id.is_empty()
        || port.proposer_reference().is_empty()
        || chrono::DateTime::parse_from_rfc3339(recorded_at).is_err()
    {
        return Err(PlanProposalRuntimeError::InvalidInput);
    }
    let ledger =
        SqliteLedger::open(database_path).map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let goal = reconstruct_goal(&ledger, session_id, goal_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    if !matches!(goal.snapshot.state(), GoalState::Draft | GoalState::Active) {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    let graph = reconstruct_plan_graph(&ledger, session_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    if graph.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    if graph.values().any(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id && definition.goal_definition_digest() == goal_digest
    }) {
        return Err(PlanProposalRuntimeError::ExistingPlanLineage);
    }
    let installation = resolve_session_installation(&ledger, session_id, &catalogue)?;
    let goal_definition = goal.snapshot.definition();
    let available_capabilities = goal_definition
        .capability_references()
        .iter()
        .map(|capability| {
            PlanCapabilityReference::new(capability.name(), capability.exact_revision())
                .map_err(|_| PlanProposalRuntimeError::CorruptState)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let request = PlanProposalRequest {
        goal_id: goal_id.into(),
        goal_revision: goal.snapshot.revision(),
        goal_definition_digest: goal_digest.clone(),
        objective: goal_definition.objective().into(),
        criterion_ids: goal_definition
            .criteria()
            .iter()
            .map(|criterion| criterion.criterion_id().as_str().to_owned())
            .collect(),
        available_capabilities: available_capabilities.clone(),
        max_total_attempts: goal_definition.bounds().max_attempts(),
    };
    let expected_session_version = goal.session_version;
    let expected_through_position = goal.through_position;
    let agent_snapshot_digest = installation.snapshot().snapshot_digest().to_owned();
    let tool_catalogue_digest = installation.tool_catalogue_digest().to_owned();
    let safety_policy_revision = installation.snapshot().governance().exact_revision.clone();
    drop(ledger);
    let content = port
        .propose(&request)
        .await
        .map_err(|_| PlanProposalRuntimeError::ProposalFailed)?;
    if content.bounds.max_total_attempts() > request.max_total_attempts {
        return Err(PlanProposalRuntimeError::ProposalFailed);
    }
    let mut ledger =
        SqliteLedger::open(database_path).map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let current = reconstruct_goal(&ledger, session_id, goal_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    if current.session_version != expected_session_version
        || current.through_position != expected_through_position
        || current.snapshot.revision() != request.goal_revision
        || current
            .snapshot
            .definition()
            .digest()
            .map_err(|_| PlanProposalRuntimeError::CorruptState)?
            != request.goal_definition_digest
    {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    if reconstruct_plan_graph(&ledger, session_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?
        .values()
        .any(|plan| plan.snapshot.definition().goal_id() == goal_id)
    {
        return Err(PlanProposalRuntimeError::ExistingPlanLineage);
    }
    let plan_id = derived_plan_id(session_id, goal_id, request.goal_revision, &goal_digest)?;
    let definition = PlanDefinitionV1::new(
        PlanId::new(&plan_id).map_err(|_| PlanProposalRuntimeError::CorruptState)?,
        1,
        goal_id,
        request.goal_revision,
        &goal_digest,
        agent_snapshot_digest,
        tool_catalogue_digest,
        safety_policy_revision,
        content.steps,
        content.bounds,
        &request.criterion_ids.iter().cloned().collect(),
        &BTreeSet::new(),
        &available_capabilities,
    )
    .map_err(|_| PlanProposalRuntimeError::ProposalFailed)?;
    let plan_definition_digest = definition
        .digest()
        .map_err(|_| PlanProposalRuntimeError::ProposalFailed)?;
    let planned = plan_propose_plan(
        &ledger,
        session_id,
        &PlanCommandContext {
            command_id: format!("g2-propose-{}", &plan_definition_digest[..32]),
            actor_reference: port.proposer_reference().into(),
            recorded_at: recorded_at.into(),
        },
        definition,
    )
    .map_err(PlanProposalRuntimeError::Plan)?;
    commit_plan_command(
        &mut ledger,
        session_id.clone(),
        expected_session_version,
        &planned,
    )
    .map_err(PlanProposalRuntimeError::Plan)?;
    Ok(ProposedGoalPlan {
        plan_id,
        plan_revision: 1,
        plan_definition_digest,
    })
}

fn resolve_session_installation<'a>(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    catalogue: &'a RuntimeAgentCatalogue,
) -> Result<&'a Arc<crate::RuntimeAgentInstallation>, PlanProposalRuntimeError> {
    let facts = ledger
        .read_facts(session_id, 0, 1, None)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let [opened] = facts.as_slice() else {
        return Err(PlanProposalRuntimeError::CorruptState);
    };
    if opened.kind.as_str() != "session.opened" {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    let value = serde_json::from_str::<serde_json::Value>(opened.payload.as_json())
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    catalogue
        .resolve(
            text(&value, "definition_id")?,
            text(&value, "definition_revision")?,
            text(&value, "snapshot_digest")?,
        )
        .map_err(|_| PlanProposalRuntimeError::CorruptState)
}

fn derived_plan_id(
    session_id: &SessionId,
    goal_id: &str,
    goal_revision: u64,
    goal_digest: &str,
) -> Result<String, PlanProposalRuntimeError> {
    let source = format!(
        "g2-plan\0{}\0{}\0{}\0{}",
        session_id.as_str(),
        goal_id,
        goal_revision,
        goal_digest
    );
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    Ok(format!("g2-plan-{}", &digest[..32]))
}

fn text<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, PlanProposalRuntimeError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(PlanProposalRuntimeError::CorruptState)
}
