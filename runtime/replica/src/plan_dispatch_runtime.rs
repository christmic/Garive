use garive_goal::GoalState;
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CommitDisposition, SessionId,
};
use garive_plan::{PlanState, PlanStepId};

use crate::{
    commit_plan_command, plan_plan_transition, plan_start_step_execution, plan_start_turn,
    reconstruct_goal, reconstruct_plan_graph, CommittedTurn, EffectiveRuntimeLimits,
    PlanCommandContext, PlanRuntimeError, PlanRuntimeTransition, PlanStepExecutionStart,
    RuntimeCommandId, SqliteLedger, StartTurnCommand, TurnDispatcher,
};

/// Explicit identities, lease readings and commands for one bounded dispatch tick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanDispatchTick {
    /// Stable Runtime worker identity.
    pub worker_reference: String,
    /// Stable claim identity reused only for the same claim semantics.
    pub claim_id: String,
    /// Positive fencing epoch.
    pub lease_epoch: u64,
    /// Named monotonic clock implementation revision.
    pub clock_revision: String,
    /// Inclusive claim observation tick.
    pub claimed_at_tick: u64,
    /// Exclusive claim expiry tick.
    pub expires_at_tick: u64,
    /// Tick proving start precedes expiry.
    pub observed_at_tick: u64,
    /// Stable logical attempt identity.
    pub attempt_id: String,
    /// Idempotent claim command identity.
    pub claim_command_id: String,
    /// Idempotent atomic C6/Step-start command identity.
    pub start_command_id: String,
    /// Canonical RFC 3339 observation time.
    pub recorded_at: String,
}

/// Fixed Runtime-owned coordinates exposed to start preparation.
pub struct PlanStepDispatchInput<'a> {
    /// Owning Session.
    pub session_id: &'a SessionId,
    /// Exact Goal identity.
    pub goal_id: &'a str,
    /// Exact Plan identity.
    pub plan_id: &'a str,
    /// Exact Plan revision.
    pub plan_revision: u64,
    /// Selected Ready/Claimed Step identity.
    pub step_id: &'a PlanStepId,
    /// Bounded Step objective used as trusted execution input.
    pub objective: &'a str,
    /// Exact Agent snapshot digest frozen by the Plan.
    pub agent_snapshot_digest: &'a str,
    /// Exact Tool catalogue digest frozen by the Plan.
    pub tool_catalogue_digest: &'a str,
    /// Exact Safety policy revision frozen by the Plan.
    pub safety_policy_revision: &'a str,
    /// Fixed prefix preceding C6 start.
    pub through_position: u64,
    /// Runtime-owned start command identity.
    pub start_command_id: &'a str,
    /// Canonical Runtime observation time.
    pub recorded_at: &'a str,
}

/// Installed Agent and execution posture resolved for one exact claimed Step.
pub struct PreparedPlanStepDispatch {
    /// Exact installed Agent instance bound to the Session.
    pub agent_instance_id: AgentInstanceId,
    /// Exact installed Agent Definition identity bound to the Session.
    pub definition_id: AgentDefinitionId,
    /// Exact installed Agent Definition revision bound to the Session.
    pub definition_revision: AgentDefinitionRevision,
    /// Frozen effective C6 limits admitted for this Agent snapshot.
    pub limits: EffectiveRuntimeLimits,
    /// Tool catalogue digest resolved from the installed execution posture.
    pub installed_tool_catalogue_digest: String,
    /// Safety policy revision resolved from the installed execution posture.
    pub installed_safety_policy_revision: String,
}

/// Explicit composition boundary for installed Agent and execution posture resolution.
pub trait PlanStepDispatchFactory {
    /// Prepares, but does not commit or dispatch, one exact Step start.
    fn prepare(
        &mut self,
        input: PlanStepDispatchInput<'_>,
    ) -> Result<PreparedPlanStepDispatch, PlanDispatchError>;
}

/// One bounded Plan dispatch result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanDispatchOutcome {
    /// No Step is currently Ready.
    NoReadyStep,
    /// A different or already-started claim owns available work.
    ClaimBusy,
    /// C6 and Step start committed; queue admission is best-effort afterward.
    Started {
        /// Exact durable worker coordinates.
        committed: CommittedTurn,
        /// Whether the process-local queue accepted the committed Turn.
        dispatch_accepted: bool,
    },
}

/// Stable secret-free Plan dispatch failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanDispatchError {
    /// Explicit tick values are malformed.
    InvalidTick,
    /// Goal/Plan projections do not share one authoritative prefix.
    CorruptState,
    /// No unique authoritative Plan is dispatchable for the Goal.
    AuthoritativePlanUnavailable,
    /// Installed Agent or execution posture preparation failed.
    PreparationFailed,
    /// Plan transition or optimistic commit failed.
    Plan(PlanRuntimeError),
}

/// Claims and starts at most one Ready Step at one verified Session prefix.
pub fn dispatch_plan_step_once(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    tick: &PlanDispatchTick,
    factory: &mut dyn PlanStepDispatchFactory,
    dispatcher: &dyn TurnDispatcher,
) -> Result<PlanDispatchOutcome, PlanDispatchError> {
    validate_tick(tick)?;
    let goal = reconstruct_goal(ledger, session_id, goal_id)
        .map_err(|_| PlanDispatchError::CorruptState)?;
    if goal.snapshot.state() != GoalState::Active {
        return Err(PlanDispatchError::AuthoritativePlanUnavailable);
    }
    let mut plan = authoritative_plan(ledger, session_id, &goal, goal_id)?;
    let step_id = if let Some((step_id, claim)) = plan
        .active_claims
        .iter()
        .find(|(_, claim)| claim.claim_id == tick.claim_id)
    {
        if claim.worker_reference != tick.worker_reference
            || claim.lease_epoch != tick.lease_epoch
            || claim.attempt_id.is_some()
        {
            return Ok(PlanDispatchOutcome::ClaimBusy);
        }
        step_id.clone()
    } else {
        let Some(step_id) = plan.snapshot.ready_steps().first().cloned().cloned() else {
            return Ok(if plan.active_claims.is_empty() {
                PlanDispatchOutcome::NoReadyStep
            } else {
                PlanDispatchOutcome::ClaimBusy
            });
        };
        let claimed = plan_plan_transition(
            &plan,
            plan.state_version,
            &context(&tick.claim_command_id, tick),
            PlanRuntimeTransition::Claim {
                step_id: step_id.clone(),
                claim_id: tick.claim_id.clone(),
                worker_reference: tick.worker_reference.clone(),
                lease_epoch: tick.lease_epoch,
                clock_revision: tick.clock_revision.clone(),
                claimed_at_tick: tick.claimed_at_tick,
                expires_at_tick: tick.expires_at_tick,
            },
        )
        .map_err(PlanDispatchError::Plan)?;
        commit_plan_command(ledger, session_id.clone(), plan.session_version, &claimed)
            .map_err(PlanDispatchError::Plan)?;
        let refreshed = reconstruct_goal(ledger, session_id, goal_id)
            .map_err(|_| PlanDispatchError::CorruptState)?;
        plan = authoritative_plan(ledger, session_id, &refreshed, goal_id)?;
        step_id
    };
    let step = plan
        .snapshot
        .definition()
        .steps()
        .iter()
        .find(|step| step.step_id() == &step_id)
        .ok_or(PlanDispatchError::CorruptState)?;
    let prepared = factory.prepare(PlanStepDispatchInput {
        session_id,
        goal_id,
        plan_id: plan.snapshot.definition().plan_id().as_str(),
        plan_revision: plan.snapshot.definition().plan_revision(),
        step_id: &step_id,
        objective: step.objective(),
        agent_snapshot_digest: plan.snapshot.definition().agent_snapshot_digest(),
        tool_catalogue_digest: plan.snapshot.definition().tool_catalogue_digest(),
        safety_policy_revision: plan.snapshot.definition().safety_policy_revision(),
        through_position: plan.through_position,
        start_command_id: &tick.start_command_id,
        recorded_at: &tick.recorded_at,
    })?;
    validate_installed_binding(ledger, session_id, plan.snapshot.definition(), &prepared)?;
    let turn = plan_start_turn(
        &StartTurnCommand {
            command_id: RuntimeCommandId::new(tick.start_command_id.as_str())
                .map_err(|_| PlanDispatchError::InvalidTick)?,
            session_id: session_id.clone(),
            agent_instance_id: prepared.agent_instance_id.clone(),
            definition_id: prepared.definition_id.clone(),
            definition_revision: prepared.definition_revision.clone(),
            snapshot_digest: plan.snapshot.definition().agent_snapshot_digest().into(),
            trusted_input: step.objective().into(),
            limits: prepared.limits,
            recorded_at: tick.recorded_at.clone(),
        },
        plan.through_position,
    )
    .map_err(|_| PlanDispatchError::PreparationFailed)?;
    let execution_id = turn
        .execution_id
        .clone()
        .ok_or(PlanDispatchError::PreparationFailed)?;
    let started = plan_start_step_execution(
        &plan,
        plan.state_version,
        &context(&tick.start_command_id, tick),
        PlanStepExecutionStart {
            step_id,
            claim_id: tick.claim_id.clone(),
            lease_epoch: tick.lease_epoch,
            clock_revision: tick.clock_revision.clone(),
            observed_at_tick: tick.observed_at_tick,
            attempt_id: tick.attempt_id.clone(),
        },
        &turn,
    )
    .map_err(PlanDispatchError::Plan)?;
    let commit = commit_plan_command(ledger, session_id.clone(), plan.session_version, &started)
        .map_err(PlanDispatchError::Plan)?;
    let committed = CommittedTurn {
        session_id: session_id.clone(),
        turn_id: turn.turn_id,
        execution_id,
        definition_id: prepared.definition_id.as_str().into(),
        definition_revision: prepared.definition_revision.as_str().into(),
        snapshot_digest: plan.snapshot.definition().agent_snapshot_digest().into(),
        session_version: commit.session_version,
        committed_position: *commit
            .positions
            .last()
            .ok_or(PlanDispatchError::CorruptState)?,
    };
    let dispatch_accepted = commit.disposition == CommitDisposition::Committed
        && dispatcher.dispatch(&committed).is_ok();
    Ok(PlanDispatchOutcome::Started {
        committed,
        dispatch_accepted,
    })
}

fn authoritative_plan(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal: &crate::GoalRuntimeState,
    goal_id: &str,
) -> Result<crate::PlanRuntimeState, PlanDispatchError> {
    let plans =
        reconstruct_plan_graph(ledger, session_id).map_err(|_| PlanDispatchError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(PlanDispatchError::CorruptState);
    }
    let digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| PlanDispatchError::CorruptState)?;
    let mut candidates = plans.into_values().filter(|plan| {
        let definition = plan.snapshot.definition();
        definition.goal_id() == goal_id
            && definition.goal_revision() <= goal.snapshot.revision()
            && definition.goal_definition_digest() == digest
            && matches!(
                plan.snapshot.state(),
                PlanState::Adopted | PlanState::Running
            )
    });
    let plan = candidates
        .next()
        .ok_or(PlanDispatchError::AuthoritativePlanUnavailable)?;
    if candidates.next().is_some() {
        return Err(PlanDispatchError::CorruptState);
    }
    Ok(plan)
}

fn validate_installed_binding(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    definition: &garive_plan::PlanDefinitionV1,
    prepared: &PreparedPlanStepDispatch,
) -> Result<(), PlanDispatchError> {
    let facts = ledger
        .read_facts(session_id, 0, 1, None)
        .map_err(|_| PlanDispatchError::CorruptState)?;
    let [opened] = facts.as_slice() else {
        return Err(PlanDispatchError::CorruptState);
    };
    let value = serde_json::from_str::<serde_json::Value>(opened.payload.as_json())
        .map_err(|_| PlanDispatchError::CorruptState)?;
    if opened.kind.as_str() != "session.opened"
        || value
            .get("definition_id")
            .and_then(serde_json::Value::as_str)
            != Some(prepared.definition_id.as_str())
        || value
            .get("definition_revision")
            .and_then(serde_json::Value::as_str)
            != Some(prepared.definition_revision.as_str())
        || value
            .get("agent_instance_id")
            .and_then(serde_json::Value::as_str)
            != Some(prepared.agent_instance_id.as_str())
        || value
            .get("snapshot_digest")
            .and_then(serde_json::Value::as_str)
            != Some(definition.agent_snapshot_digest())
        || prepared.installed_tool_catalogue_digest != definition.tool_catalogue_digest()
        || prepared.installed_safety_policy_revision != definition.safety_policy_revision()
    {
        return Err(PlanDispatchError::PreparationFailed);
    }
    Ok(())
}

fn validate_tick(tick: &PlanDispatchTick) -> Result<(), PlanDispatchError> {
    if [
        tick.worker_reference.as_str(),
        tick.claim_id.as_str(),
        tick.clock_revision.as_str(),
        tick.attempt_id.as_str(),
        tick.claim_command_id.as_str(),
        tick.start_command_id.as_str(),
    ]
    .iter()
    .any(|value| value.is_empty())
        || tick.lease_epoch == 0
        || tick.expires_at_tick <= tick.claimed_at_tick
        || tick.observed_at_tick < tick.claimed_at_tick
        || tick.observed_at_tick >= tick.expires_at_tick
        || tick.claim_command_id == tick.start_command_id
        || chrono::DateTime::parse_from_rfc3339(&tick.recorded_at).is_err()
    {
        return Err(PlanDispatchError::InvalidTick);
    }
    Ok(())
}

fn context(command_id: &str, tick: &PlanDispatchTick) -> PlanCommandContext {
    PlanCommandContext {
        command_id: command_id.into(),
        actor_reference: tick.worker_reference.clone(),
        recorded_at: tick.recorded_at.clone(),
    }
}
