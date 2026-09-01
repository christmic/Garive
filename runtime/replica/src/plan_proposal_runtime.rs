//! Fixed-prefix construction of one initial executable Plan proposal.

use std::{collections::BTreeSet, future::Future, path::Path, pin::Pin, sync::Arc};

use garive_goal::GoalState;
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, DurableFact,
    ExecutionId, SessionId, TurnId,
};
use garive_plan::{PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanStepV1};
use sha2::{Digest, Sha256};

use crate::{
    commit_plan_command, commit_planned_turn, plan_proposal_output_schema, plan_propose_plan,
    plan_start_plan_proposal_execution, reconstruct_goal, reconstruct_plan_graph, CommittedTurn,
    PlanCommandContext, PlanRuntimeError, RuntimeAgentCatalogue, RuntimeCommandId, SqliteLedger,
    StartPlanProposalExecutionCommand, StartTurnCommand,
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

/// Starts or reconstructs one durable model-backed initial proposal Execution.
pub fn start_initial_goal_plan_proposal_execution(
    database_path: &Path,
    session_id: &SessionId,
    goal_id: &str,
    proposer_reference: &str,
    recorded_at: &str,
    catalogue: Arc<RuntimeAgentCatalogue>,
) -> Result<CommittedTurn, PlanProposalRuntimeError> {
    if goal_id.is_empty()
        || proposer_reference.is_empty()
        || chrono::DateTime::parse_from_rfc3339(recorded_at).is_err()
    {
        return Err(PlanProposalRuntimeError::InvalidInput);
    }
    let mut ledger =
        SqliteLedger::open(database_path).map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let goal = reconstruct_goal(&ledger, session_id, goal_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    if !matches!(goal.snapshot.state(), GoalState::Draft | GoalState::Active) {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let facts = ledger
        .read_facts(session_id, 0, goal.through_position, None)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let opened = session_opened(&facts)?;
    let opened_value = serde_json::from_str::<serde_json::Value>(opened.payload.as_json())
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let installation = catalogue
        .resolve(
            text(&opened_value, "definition_id")?,
            text(&opened_value, "definition_revision")?,
            text(&opened_value, "snapshot_digest")?,
        )
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let existing = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.proposal.requested")
        .filter_map(|fact| {
            serde_json::from_str::<serde_json::Value>(fact.payload.as_json())
                .ok()
                .filter(|value| {
                    value.get("goal_id").and_then(serde_json::Value::as_str) == Some(goal_id)
                        && value
                            .get("goal_revision")
                            .and_then(serde_json::Value::as_u64)
                            == Some(goal.snapshot.revision())
                        && value
                            .get("goal_definition_digest")
                            .and_then(serde_json::Value::as_str)
                            == Some(goal_digest.as_str())
                })
                .map(|value| (fact, value))
        })
        .collect::<Vec<_>>();
    if let [(fact, value)] = existing.as_slice() {
        if text(value, "proposer_reference")? != proposer_reference {
            return Err(PlanProposalRuntimeError::CorruptState);
        }
        return existing_committed_turn(&ledger, session_id, &facts, fact, value, &opened_value);
    }
    if !existing.is_empty() {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    if reconstruct_plan_graph(&ledger, session_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?
        .values()
        .any(|plan| plan.snapshot.definition().goal_id() == goal_id)
    {
        return Err(PlanProposalRuntimeError::ExistingPlanLineage);
    }
    let definition = goal.snapshot.definition();
    let schema = plan_proposal_output_schema();
    let prompt = CanonicalPayload::from_value(&serde_json::json!({
        "contract":"garive.plan-proposal-request", "version":1,
        "goal":{"goal_id":goal_id,"goal_revision":goal.snapshot.revision(),
            "goal_definition_digest":goal_digest,"objective":definition.objective(),
            "criterion_ids":definition.criteria().iter().map(|value| value.criterion_id().as_str()).collect::<Vec<_>>(),
            "available_capabilities":definition.capability_references().iter().map(|value| serde_json::json!({"name":value.name(),"exact_revision":value.exact_revision()})).collect::<Vec<_>>(),
            "max_total_attempts":definition.bounds().max_attempts()},
        "output":{"contract":"garive.plan-proposal-topology","version":1,
            "schema_digest":schema.digest}
    }))
    .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let command_seed = format!("{}:{}:{}", goal_id, goal.snapshot.revision(), goal_digest);
    let planned = plan_start_plan_proposal_execution(
        &StartPlanProposalExecutionCommand {
            start: StartTurnCommand {
                command_id: RuntimeCommandId::new(format!(
                    "planner-start-{}",
                    &digest(command_seed.as_bytes())[..32]
                ))
                .map_err(|_| PlanProposalRuntimeError::InvalidInput)?,
                session_id: session_id.clone(),
                agent_instance_id: AgentInstanceId::try_from(text(
                    &opened_value,
                    "agent_instance_id",
                )?)
                .map_err(|_| PlanProposalRuntimeError::CorruptState)?,
                definition_id: AgentDefinitionId::try_from(text(&opened_value, "definition_id")?)
                    .map_err(|_| PlanProposalRuntimeError::CorruptState)?,
                definition_revision: AgentDefinitionRevision::try_from(text(
                    &opened_value,
                    "definition_revision",
                )?)
                .map_err(|_| PlanProposalRuntimeError::CorruptState)?,
                snapshot_digest: installation.snapshot().snapshot_digest().into(),
                trusted_input: prompt.as_json().into(),
                limits: installation.installed_agent().runtime_limits,
                recorded_at: recorded_at.into(),
            },
            goal_id: goal_id.into(),
            goal_revision: goal.snapshot.revision(),
            goal_definition_digest: goal_digest,
            expected_session_version: goal.session_version,
            proposer_reference: proposer_reference.into(),
            output_schema_digest: schema.digest,
        },
        goal.through_position,
    )
    .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let result = commit_planned_turn(
        &mut ledger,
        session_id.clone(),
        goal.session_version,
        &planned,
    )
    .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    Ok(CommittedTurn {
        session_id: session_id.clone(),
        turn_id: planned.turn_id,
        execution_id: planned
            .execution_id
            .ok_or(PlanProposalRuntimeError::CorruptState)?,
        definition_id: text(&opened_value, "definition_id")?.into(),
        definition_revision: text(&opened_value, "definition_revision")?.into(),
        snapshot_digest: installation.snapshot().snapshot_digest().into(),
        session_version: result.session_version,
        committed_position: *result
            .positions
            .last()
            .ok_or(PlanProposalRuntimeError::CorruptState)?,
    })
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

fn session_opened(facts: &[DurableFact]) -> Result<&DurableFact, PlanProposalRuntimeError> {
    let mut opened = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "session.opened");
    let value = opened
        .next()
        .ok_or(PlanProposalRuntimeError::CorruptState)?;
    if opened.next().is_some() {
        Err(PlanProposalRuntimeError::CorruptState)
    } else {
        Ok(value)
    }
}

fn existing_committed_turn(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    facts: &[DurableFact],
    request: &DurableFact,
    request_value: &serde_json::Value,
    opened_value: &serde_json::Value,
) -> Result<CommittedTurn, PlanProposalRuntimeError> {
    let turn_id = TurnId::try_from(text(request_value, "turn_id")?)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let execution_id = ExecutionId::try_from(text(request_value, "execution_id")?)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?;
    let positions = [
        ("turn.started", request.position.checked_add(1)),
        ("turn.input", request.position.checked_add(2)),
        ("execution.started", request.position.checked_add(3)),
    ];
    for (kind, position) in positions {
        let position = position.ok_or(PlanProposalRuntimeError::CorruptState)?;
        let fact = facts
            .iter()
            .find(|fact| fact.position == position && fact.kind.as_str() == kind)
            .ok_or(PlanProposalRuntimeError::CorruptState)?;
        if fact.turn_id.as_ref() != Some(&turn_id)
            || kind == "execution.started" && fact.execution_id.as_ref() != Some(&execution_id)
        {
            return Err(PlanProposalRuntimeError::CorruptState);
        }
    }
    let session_version = ledger
        .fact_commit_version(&request.fact_id)
        .map_err(|_| PlanProposalRuntimeError::CorruptState)?
        .ok_or(PlanProposalRuntimeError::CorruptState)?;
    if request_value
        .get("expected_session_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| value.checked_add(1))
        != Some(session_version)
    {
        return Err(PlanProposalRuntimeError::CorruptState);
    }
    Ok(CommittedTurn {
        session_id: session_id.clone(),
        turn_id,
        execution_id,
        definition_id: text(opened_value, "definition_id")?.into(),
        definition_revision: text(opened_value, "definition_revision")?.into(),
        snapshot_digest: text(opened_value, "snapshot_digest")?.into(),
        session_version,
        committed_position: request
            .position
            .checked_add(3)
            .ok_or(PlanProposalRuntimeError::CorruptState)?,
    })
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
