//! Fixed-prefix Goal/Plan coordination decisions.

use garive_goal::GoalState;
use garive_ledger::{CanonicalPayload, SessionId};
use garive_plan::{PlanState, PlanStepId, StepState};
use sha2::{Digest, Sha256};

use crate::{
    commit_goal_command, commit_plan_command, dispatch_plan_step_once,
    plan_activate_goal_from_authoritative_plan, plan_complete_authoritative_plan,
    plan_fail_goal_from_failed_plan, plan_succeed_goal_from_completed_plan, reconstruct_goal,
    reconstruct_plan_graph, GoalCommandContext, GoalPlanCoordinationError, GoalRuntimeError,
    PlanCommandContext, PlanDispatchError, PlanDispatchOutcome, PlanDispatchTick, PlanRuntimeError,
    PlanStepDispatchFactory, SqliteLedger, TurnDispatcher,
};

/// One bounded Runtime coordination decision over a verified Session prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoalPlanDecision {
    /// The Goal/Plan lineage currently needs no coordinator mutation.
    NoAction,
    /// A planning policy must produce a bounded Plan proposal.
    ProposePlan,
    /// One exact proposal is waiting for an injected admission policy.
    AdmitProposedPlan {
        /// Exact proposed Plan identity.
        plan_id: String,
        /// Exact proposed Plan revision.
        plan_revision: u64,
    },
    /// The adopted Plan may activate its exact Draft Goal revision.
    ActivateGoal {
        /// Exact authoritative Plan identity.
        plan_id: String,
        /// Exact authoritative Plan revision.
        plan_revision: u64,
    },
    /// The first declaration-ordered Ready Step may enter bounded dispatch.
    DispatchReadyStep {
        /// Exact authoritative Plan identity.
        plan_id: String,
        /// Exact authoritative Plan revision.
        plan_revision: u64,
        /// Runtime-selected Ready Step.
        step_id: PlanStepId,
    },
    /// Every Step is complete and Runtime may verify the Plan terminal.
    CompletePlan {
        /// Exact authoritative Plan identity.
        plan_id: String,
        /// Exact authoritative Plan revision.
        plan_revision: u64,
    },
    /// A unique completed Plan may be independently reduced to Goal success.
    SucceedGoal {
        /// Exact completed Plan identity.
        plan_id: String,
        /// Exact completed Plan revision.
        plan_revision: u64,
    },
    /// A unique failed Plan may be independently reduced to Goal failure.
    FailGoal {
        /// Exact failed Plan identity.
        plan_id: String,
        /// Exact failed Plan revision.
        plan_revision: u64,
    },
    /// Runtime cannot progress without an admitted policy or reconciliation.
    NeedsOperator {
        /// Stable secret-free reason code.
        reason: &'static str,
    },
}

/// Frozen coordinates and exactly one decision from a single ledger watermark.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalPlanCoordinationSnapshot {
    /// Highest durable position included in every reconstructed projection.
    pub through_position: u64,
    /// Session version at the same durable prefix.
    pub session_version: u64,
    /// Exact Goal identity.
    pub goal_id: String,
    /// Current Goal lifecycle revision.
    pub goal_revision: u64,
    /// Canonical immutable Goal definition digest.
    pub goal_definition_digest: String,
    /// Single bounded coordination decision.
    pub decision: GoalPlanDecision,
}

/// Explicit Runtime-owned metadata for one coordination attempt.
pub struct GoalPlanCoordinationTick {
    /// Stable internal coordinator identity.
    pub actor_reference: String,
    /// Canonical RFC 3339 observation time.
    pub recorded_at: String,
    /// Fenced worker tick, required only for `DispatchReadyStep`.
    pub dispatch: Option<PlanDispatchTick>,
}

/// Read-only fixed-prefix input exposed to a Plan admission policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanAdmissionInput {
    /// Durable Session version that fences the decision.
    pub session_version: u64,
    /// Highest durable position visible to the decision.
    pub through_position: u64,
    /// Exact Goal identity.
    pub goal_id: String,
    /// Exact Goal revision.
    pub goal_revision: u64,
    /// Canonical immutable Goal definition digest.
    pub goal_definition_digest: String,
    /// Exact proposed Plan identity.
    pub plan_id: String,
    /// Exact proposed Plan revision.
    pub plan_revision: u64,
    /// Canonical immutable Plan definition digest.
    pub plan_definition_digest: String,
}

/// Bounded result returned by a constructed Plan admission policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanAdmissionDecision {
    /// Adopt the exact proposal under this policy revision.
    Adopt {
        /// Stable policy revision that grants adoption.
        policy_reference: String,
    },
    /// Reject the exact proposal with a stable secret-free reason.
    Reject {
        /// Stable policy revision that denies adoption.
        policy_reference: String,
        /// Stable secret-free reason code.
        reason: String,
    },
    /// Preserve the proposal without granting authority.
    Defer {
        /// Stable secret-free reason code.
        reason: String,
    },
}

/// Read-only policy boundary for one exact proposed Plan.
pub trait PlanAdmissionPolicy: Send + Sync {
    /// Decides without receiving a Ledger or any mutation capability.
    fn decide(&self, input: &PlanAdmissionInput) -> PlanAdmissionDecision;
}

/// Result of advancing at most one coordination decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoalPlanAdvanceOutcome {
    /// The fixed prefix requires no mutation.
    NoAction,
    /// A planning, admission, continuation or failure policy must decide next.
    AwaitingPolicy(GoalPlanDecision),
    /// One non-dispatch coordination command committed durably.
    Committed(GoalPlanDecision),
    /// The bounded Step dispatcher made its one allowed decision.
    Dispatch(PlanDispatchOutcome),
}

/// Stable failures from one bounded coordination advance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalPlanCoordinatorError {
    /// Runtime-owned metadata is malformed or incomplete.
    InvalidTick,
    /// Fixed-prefix evaluation failed closed.
    Coordination(GoalPlanCoordinationError),
    /// Goal command planning or durability failed.
    Goal(GoalRuntimeError),
    /// Plan command planning or durability failed.
    Plan(PlanRuntimeError),
    /// Bounded Step dispatch failed.
    Dispatch(PlanDispatchError),
}

/// Evaluates one Goal lineage without mutating durable state.
pub fn evaluate_goal_plan_once(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
) -> Result<GoalPlanCoordinationSnapshot, GoalPlanCoordinationError> {
    let goal =
        reconstruct_goal(ledger, session_id, goal_id).map_err(GoalPlanCoordinationError::Goal)?;
    let plans = reconstruct_plan_graph(ledger, session_id)
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if plans.values().any(|plan| {
        plan.session_version != goal.session_version
            || plan.through_position != goal.through_position
    }) {
        return Err(GoalPlanCoordinationError::ConcurrentModification);
    }
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let related = plans
        .values()
        .filter(|plan| {
            let definition = plan.snapshot.definition();
            definition.goal_id() == goal_id
                && definition.goal_definition_digest() == goal_digest
                && definition.goal_revision() <= goal.snapshot.revision()
        })
        .collect::<Vec<_>>();
    let decision = decide(&goal, &related)?;
    Ok(GoalPlanCoordinationSnapshot {
        through_position: goal.through_position,
        session_version: goal.session_version,
        goal_id: goal_id.into(),
        goal_revision: goal.snapshot.revision(),
        goal_definition_digest: goal_digest,
        decision,
    })
}

/// Evaluates and commits at most one policy-independent coordination action.
pub fn advance_goal_plan_once(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    tick: &GoalPlanCoordinationTick,
    factory: &mut dyn PlanStepDispatchFactory,
    dispatcher: &dyn TurnDispatcher,
) -> Result<GoalPlanAdvanceOutcome, GoalPlanCoordinatorError> {
    advance_goal_plan_internal(ledger, session_id, goal_id, tick, factory, dispatcher, None)
}

/// Advances one Goal lineage with an explicitly constructed admission policy.
pub fn advance_goal_plan_with_admission_once(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    tick: &GoalPlanCoordinationTick,
    factory: &mut dyn PlanStepDispatchFactory,
    dispatcher: &dyn TurnDispatcher,
    admission_policy: &dyn PlanAdmissionPolicy,
) -> Result<GoalPlanAdvanceOutcome, GoalPlanCoordinatorError> {
    advance_goal_plan_internal(
        ledger,
        session_id,
        goal_id,
        tick,
        factory,
        dispatcher,
        Some(admission_policy),
    )
}

fn advance_goal_plan_internal(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    tick: &GoalPlanCoordinationTick,
    factory: &mut dyn PlanStepDispatchFactory,
    dispatcher: &dyn TurnDispatcher,
    admission_policy: Option<&dyn PlanAdmissionPolicy>,
) -> Result<GoalPlanAdvanceOutcome, GoalPlanCoordinatorError> {
    if tick.actor_reference.is_empty()
        || chrono::DateTime::parse_from_rfc3339(&tick.recorded_at).is_err()
    {
        return Err(GoalPlanCoordinatorError::InvalidTick);
    }
    let snapshot = evaluate_goal_plan_once(ledger, session_id, goal_id)
        .map_err(GoalPlanCoordinatorError::Coordination)?;
    let decision = snapshot.decision.clone();
    match &decision {
        GoalPlanDecision::NoAction => Ok(GoalPlanAdvanceOutcome::NoAction),
        GoalPlanDecision::ProposePlan | GoalPlanDecision::NeedsOperator { .. } => {
            Ok(GoalPlanAdvanceOutcome::AwaitingPolicy(decision))
        }
        GoalPlanDecision::AdmitProposedPlan {
            plan_id,
            plan_revision,
        } => {
            let Some(policy) = admission_policy else {
                return Ok(GoalPlanAdvanceOutcome::AwaitingPolicy(decision));
            };
            let plans = reconstruct_plan_graph(ledger, session_id).map_err(|_| {
                GoalPlanCoordinatorError::Coordination(GoalPlanCoordinationError::CorruptState)
            })?;
            let proposal = plans
                .values()
                .find(|plan| {
                    let definition = plan.snapshot.definition();
                    definition.plan_id().as_str() == plan_id
                        && definition.plan_revision() == *plan_revision
                        && plan.snapshot.state() == PlanState::Proposed
                        && plan.session_version == snapshot.session_version
                        && plan.through_position == snapshot.through_position
                })
                .ok_or(GoalPlanCoordinatorError::Coordination(
                    GoalPlanCoordinationError::ConcurrentModification,
                ))?;
            let plan_definition_digest = proposal.snapshot.definition().digest().map_err(|_| {
                GoalPlanCoordinatorError::Coordination(GoalPlanCoordinationError::CorruptState)
            })?;
            let policy_decision = policy.decide(&PlanAdmissionInput {
                session_version: snapshot.session_version,
                through_position: snapshot.through_position,
                goal_id: snapshot.goal_id.clone(),
                goal_revision: snapshot.goal_revision,
                goal_definition_digest: snapshot.goal_definition_digest.clone(),
                plan_id: plan_id.clone(),
                plan_revision: *plan_revision,
                plan_definition_digest,
            });
            let context = plan_context(&snapshot, tick, "admit-plan");
            let planned = match policy_decision {
                PlanAdmissionDecision::Adopt { policy_reference } => crate::plan_adopt_plan(
                    ledger,
                    session_id,
                    proposal,
                    proposal.state_version,
                    &context,
                    crate::PlanRuntimeTransition::Adopt {
                        expected_goal_revision: snapshot.goal_revision,
                        expected_prior_plan_revision: None,
                        policy_reference,
                        carry_forward_evidence: CanonicalPayload::from_value(
                            &serde_json::json!([]),
                        )
                        .map_err(|_| {
                            GoalPlanCoordinatorError::Coordination(
                                GoalPlanCoordinationError::CorruptState,
                            )
                        })?,
                    },
                )
                .map_err(GoalPlanCoordinatorError::Plan)?,
                PlanAdmissionDecision::Reject {
                    policy_reference,
                    reason,
                } => crate::plan_reject_plan(
                    ledger,
                    session_id,
                    proposal,
                    proposal.state_version,
                    &context,
                    policy_reference,
                    reason,
                )
                .map_err(GoalPlanCoordinatorError::Plan)?,
                PlanAdmissionDecision::Defer { .. } => {
                    return Ok(GoalPlanAdvanceOutcome::AwaitingPolicy(decision));
                }
            };
            commit_plan_command(
                ledger,
                session_id.clone(),
                snapshot.session_version,
                &planned,
            )
            .map_err(GoalPlanCoordinatorError::Plan)?;
            Ok(GoalPlanAdvanceOutcome::Committed(decision))
        }
        GoalPlanDecision::ActivateGoal { .. } => {
            let context = goal_context(&snapshot, tick, "activate");
            let planned = plan_activate_goal_from_authoritative_plan(
                ledger,
                session_id,
                goal_id,
                snapshot.session_version,
                snapshot.goal_revision,
                &context,
            )
            .map_err(GoalPlanCoordinatorError::Coordination)?;
            commit_goal_command(
                ledger,
                session_id.clone(),
                snapshot.session_version,
                &planned,
            )
            .map_err(GoalPlanCoordinatorError::Goal)?;
            Ok(GoalPlanAdvanceOutcome::Committed(decision))
        }
        GoalPlanDecision::DispatchReadyStep { .. } => {
            let dispatch = tick
                .dispatch
                .as_ref()
                .ok_or(GoalPlanCoordinatorError::InvalidTick)?;
            dispatch_plan_step_once(ledger, session_id, goal_id, dispatch, factory, dispatcher)
                .map(GoalPlanAdvanceOutcome::Dispatch)
                .map_err(GoalPlanCoordinatorError::Dispatch)
        }
        GoalPlanDecision::CompletePlan { .. } => {
            let context = plan_context(&snapshot, tick, "complete-plan");
            let planned = plan_complete_authoritative_plan(
                ledger,
                session_id,
                goal_id,
                snapshot.session_version,
                snapshot.goal_revision,
                &context,
            )
            .map_err(GoalPlanCoordinatorError::Coordination)?
            .ok_or(GoalPlanCoordinatorError::Coordination(
                GoalPlanCoordinationError::CorruptState,
            ))?;
            commit_plan_command(
                ledger,
                session_id.clone(),
                snapshot.session_version,
                &planned,
            )
            .map_err(GoalPlanCoordinatorError::Plan)?;
            Ok(GoalPlanAdvanceOutcome::Committed(decision))
        }
        GoalPlanDecision::SucceedGoal { .. } => {
            let context = goal_context(&snapshot, tick, "succeed-goal");
            let planned = plan_succeed_goal_from_completed_plan(
                ledger,
                session_id,
                goal_id,
                snapshot.session_version,
                snapshot.goal_revision,
                &context,
            )
            .map_err(GoalPlanCoordinatorError::Coordination)?;
            commit_goal_command(
                ledger,
                session_id.clone(),
                snapshot.session_version,
                &planned,
            )
            .map_err(GoalPlanCoordinatorError::Goal)?;
            Ok(GoalPlanAdvanceOutcome::Committed(decision))
        }
        GoalPlanDecision::FailGoal { .. } => {
            let context = goal_context(&snapshot, tick, "fail-goal");
            let planned = plan_fail_goal_from_failed_plan(
                ledger,
                session_id,
                goal_id,
                snapshot.session_version,
                snapshot.goal_revision,
                &context,
            )
            .map_err(GoalPlanCoordinatorError::Coordination)?;
            commit_goal_command(
                ledger,
                session_id.clone(),
                snapshot.session_version,
                &planned,
            )
            .map_err(GoalPlanCoordinatorError::Goal)?;
            Ok(GoalPlanAdvanceOutcome::Committed(decision))
        }
    }
}

fn goal_context(
    snapshot: &GoalPlanCoordinationSnapshot,
    tick: &GoalPlanCoordinationTick,
    action: &str,
) -> GoalCommandContext {
    GoalCommandContext {
        command_id: coordination_command_id(snapshot, action),
        actor_reference: tick.actor_reference.clone(),
        recorded_at: tick.recorded_at.clone(),
    }
}

fn plan_context(
    snapshot: &GoalPlanCoordinationSnapshot,
    tick: &GoalPlanCoordinationTick,
    action: &str,
) -> PlanCommandContext {
    PlanCommandContext {
        command_id: coordination_command_id(snapshot, action),
        actor_reference: tick.actor_reference.clone(),
        recorded_at: tick.recorded_at.clone(),
    }
}

fn coordination_command_id(snapshot: &GoalPlanCoordinationSnapshot, action: &str) -> String {
    let source = format!(
        "g2\0{}\0{}\0{}\0{}\0{}",
        snapshot.goal_id,
        snapshot.goal_revision,
        snapshot.session_version,
        snapshot.through_position,
        action
    );
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    format!("g2-{}", &digest[..32])
}

fn decide(
    goal: &crate::GoalRuntimeState,
    plans: &[&crate::PlanRuntimeState],
) -> Result<GoalPlanDecision, GoalPlanCoordinationError> {
    if matches!(
        goal.snapshot.state(),
        GoalState::Succeeded | GoalState::Failed | GoalState::Cancelled
    ) {
        return Ok(GoalPlanDecision::NoAction);
    }
    let authoritative = plans
        .iter()
        .copied()
        .filter(|plan| {
            matches!(
                plan.snapshot.state(),
                PlanState::Adopted | PlanState::Running | PlanState::Suspended
            )
        })
        .collect::<Vec<_>>();
    if authoritative.len() > 1 {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    if let Some(plan) = authoritative.first().copied() {
        return decide_authoritative(goal, plan);
    }
    let terminals = plans
        .iter()
        .copied()
        .filter(|plan| {
            matches!(
                plan.snapshot.state(),
                PlanState::Completed | PlanState::Failed
            )
        })
        .collect::<Vec<_>>();
    if terminals.len() > 1 {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    if let Some(plan) = terminals.first().copied() {
        let definition = plan.snapshot.definition();
        return Ok(match plan.snapshot.state() {
            PlanState::Completed if goal.snapshot.state() == GoalState::Active => {
                GoalPlanDecision::SucceedGoal {
                    plan_id: definition.plan_id().as_str().into(),
                    plan_revision: definition.plan_revision(),
                }
            }
            PlanState::Failed if goal.snapshot.state() == GoalState::Active => {
                GoalPlanDecision::FailGoal {
                    plan_id: definition.plan_id().as_str().into(),
                    plan_revision: definition.plan_revision(),
                }
            }
            _ => GoalPlanDecision::NoAction,
        });
    }
    let proposed = plans
        .iter()
        .copied()
        .filter(|plan| plan.snapshot.state() == PlanState::Proposed)
        .collect::<Vec<_>>();
    Ok(
        if proposed.is_empty() && goal.snapshot.state() == GoalState::Draft {
            GoalPlanDecision::ProposePlan
        } else if proposed.len() == 1 {
            let definition = proposed[0].snapshot.definition();
            GoalPlanDecision::AdmitProposedPlan {
                plan_id: definition.plan_id().as_str().into(),
                plan_revision: definition.plan_revision(),
            }
        } else if proposed.len() > 1 {
            GoalPlanDecision::NeedsOperator {
                reason: "ambiguous_plan_proposals",
            }
        } else {
            GoalPlanDecision::NeedsOperator {
                reason: "authoritative_plan_unavailable",
            }
        },
    )
}

fn decide_authoritative(
    goal: &crate::GoalRuntimeState,
    plan: &crate::PlanRuntimeState,
) -> Result<GoalPlanDecision, GoalPlanCoordinationError> {
    let definition = plan.snapshot.definition();
    let plan_id = definition.plan_id().as_str().to_owned();
    let plan_revision = definition.plan_revision();
    if goal.snapshot.state() == GoalState::Draft {
        return Ok(GoalPlanDecision::ActivateGoal {
            plan_id,
            plan_revision,
        });
    }
    if goal.snapshot.state() == GoalState::Suspended {
        return Ok(GoalPlanDecision::NoAction);
    }
    if plan.snapshot.state() == PlanState::Suspended {
        return Ok(GoalPlanDecision::NeedsOperator {
            reason: "plan_continuation_required",
        });
    }
    if plan.snapshot.definition().steps().iter().all(|step| {
        plan.snapshot
            .step(step.step_id())
            .map(|value| value.state())
            == Some(StepState::Completed)
    }) {
        return Ok(GoalPlanDecision::CompletePlan {
            plan_id,
            plan_revision,
        });
    }
    if plan
        .active_claims
        .values()
        .any(|claim| claim.attempt_id.is_some())
    {
        return Ok(GoalPlanDecision::NoAction);
    }
    if let Some(step_id) = plan
        .snapshot
        .definition()
        .steps()
        .iter()
        .map(|step| step.step_id())
        .find(|step_id| plan.active_claims.contains_key(*step_id))
        .cloned()
    {
        return Ok(GoalPlanDecision::DispatchReadyStep {
            plan_id,
            plan_revision,
            step_id,
        });
    }
    if let Some(step_id) = plan.snapshot.ready_steps().first().copied().cloned() {
        return Ok(GoalPlanDecision::DispatchReadyStep {
            plan_id,
            plan_revision,
            step_id,
        });
    }
    Ok(
        if plan.snapshot.definition().steps().iter().any(|step| {
            plan.snapshot
                .step(step.step_id())
                .map(|value| value.state())
                == Some(StepState::Failed)
        }) {
            GoalPlanDecision::NeedsOperator {
                reason: "plan_failure_policy_required",
            }
        } else {
            GoalPlanDecision::NoAction
        },
    )
}
