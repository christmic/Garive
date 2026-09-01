//! Fixed-prefix Goal/Plan coordination decisions.

use garive_goal::GoalState;
use garive_ledger::{CanonicalPayload, DurableFact, FactDraft, FactId, FactKind, SessionId};
use garive_plan::{PlanState, PlanStepId, StepState};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::plan_runtime::{plan_resume_failed_plan, plan_suspend_failed_plan};
use crate::{
    commit_goal_command, commit_plan_command, commit_plan_replacement, dispatch_plan_step_once,
    plan_activate_goal_from_authoritative_plan, plan_complete_authoritative_plan,
    plan_fail_goal_from_failed_plan, plan_succeed_goal_from_completed_plan, reconstruct_goal,
    reconstruct_plan_graph, verify_plan_carry_forward, GoalCommandContext,
    GoalPlanCoordinationError, GoalRuntimeError, GoalRuntimeTransition, PlanCommandContext,
    PlanDispatchError, PlanDispatchOutcome, PlanDispatchTick, PlanRuntimeError,
    PlanStepDispatchFactory, SqliteLedger, TurnDispatcher,
};

/// One bounded Runtime coordination decision over a verified Session prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoalPlanDecision {
    /// The Goal/Plan lineage currently needs no coordinator mutation.
    NoAction,
    /// A planning policy must produce a bounded Plan proposal.
    ProposePlan,
    /// One durable failure-policy admission is waiting for revision N+1 planning.
    ProposeReplacement {
        /// Exact source Plan identity.
        source_plan_id: String,
        /// Exact source Plan revision.
        source_plan_revision: u64,
        /// Durable replan admission fact identity.
        admission_fact_id: String,
    },
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
    /// Failed work requires an explicit terminal or replanning policy.
    ResolveFailedPlan {
        /// Exact authoritative Plan identity.
        plan_id: String,
        /// Exact authoritative Plan revision.
        plan_revision: u64,
        /// Failed Step identities in declaration order.
        failed_step_ids: Vec<String>,
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
    /// Exact authoritative revision replaced by this proposal, when any.
    pub prior_plan_revision: Option<u64>,
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

/// Fixed-prefix input exposed to a Plan failure policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanFailureInput {
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
    /// Exact authoritative Plan identity.
    pub plan_id: String,
    /// Exact authoritative Plan revision.
    pub plan_revision: u64,
    /// Canonical immutable Plan definition digest.
    pub plan_definition_digest: String,
    /// Failed Step identities in declaration order.
    pub failed_step_ids: Vec<String>,
    /// Exact policy continuation when this failed Plan is suspended.
    pub suspension_reference: Option<String>,
}

/// Bounded result returned by a constructed Plan failure policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanFailureDecision {
    /// Suspend the exact failed Plan under one stable policy revision.
    Suspend {
        /// Stable policy revision granting suspension.
        policy_reference: String,
        /// Stable secret-free suspension reason code.
        reason: String,
    },
    /// Resume the exact policy-suspended failed Plan.
    Resume {
        /// Same stable policy revision that granted suspension.
        policy_reference: String,
    },
    /// Request a new immutable Plan revision under one stable policy revision.
    Replan {
        /// Stable policy revision granting replanning.
        policy_reference: String,
    },
    /// Terminalize the exact Plan under one stable policy revision.
    Fail {
        /// Stable policy revision granting terminal failure.
        policy_reference: String,
        /// Stable secret-free terminal reason code.
        reason: String,
    },
    /// Preserve the failed Plan pending another policy decision.
    Defer {
        /// Stable secret-free reason code.
        reason: String,
    },
}

/// Read-only policy boundary for one exact failed Plan prefix.
pub trait PlanFailurePolicy: Send + Sync {
    /// Decides without receiving a Ledger or mutation capability.
    fn decide(&self, input: &PlanFailureInput) -> PlanFailureDecision;
}

#[derive(Default)]
struct CoordinatorPolicies<'a> {
    admission: Option<&'a dyn PlanAdmissionPolicy>,
    failure: Option<&'a dyn PlanFailurePolicy>,
}

/// Result of advancing at most one coordination decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoalPlanAdvanceOutcome {
    /// The fixed prefix requires no mutation.
    NoAction,
    /// A planning, admission, continuation or failure policy must decide next.
    AwaitingPolicy(GoalPlanDecision),
    /// Failure policy admitted replanning without granting proposal authority.
    ReplanRequested {
        /// Fixed-prefix failed Plan decision.
        decision: GoalPlanDecision,
        /// Durable policy-admission fact committed before proposal work.
        admission_fact_id: String,
        /// Stable policy revision granting the proposal attempt.
        policy_reference: String,
    },
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
    let mut decision = decide(&goal, &related)?;
    if let GoalPlanDecision::ResolveFailedPlan {
        plan_id,
        plan_revision,
        failed_step_ids,
    } = &decision
    {
        let facts = ledger
            .read_facts(session_id, 0, goal.through_position, None)
            .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
        if let Some(admission_fact_id) =
            pending_replan_admission(&facts, &goal, plan_id, *plan_revision, failed_step_ids)?
        {
            decision = GoalPlanDecision::ProposeReplacement {
                source_plan_id: plan_id.clone(),
                source_plan_revision: *plan_revision,
                admission_fact_id,
            };
        }
    }
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
    advance_goal_plan_internal(
        ledger,
        session_id,
        goal_id,
        tick,
        factory,
        dispatcher,
        CoordinatorPolicies::default(),
    )
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
        CoordinatorPolicies {
            admission: Some(admission_policy),
            failure: None,
        },
    )
}

/// Advances one Goal lineage with an explicitly constructed failure policy.
pub fn advance_goal_plan_with_failure_once(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    tick: &GoalPlanCoordinationTick,
    factory: &mut dyn PlanStepDispatchFactory,
    dispatcher: &dyn TurnDispatcher,
    failure_policy: &dyn PlanFailurePolicy,
) -> Result<GoalPlanAdvanceOutcome, GoalPlanCoordinatorError> {
    advance_goal_plan_internal(
        ledger,
        session_id,
        goal_id,
        tick,
        factory,
        dispatcher,
        CoordinatorPolicies {
            admission: None,
            failure: Some(failure_policy),
        },
    )
}

fn advance_goal_plan_internal(
    ledger: &mut SqliteLedger,
    session_id: &SessionId,
    goal_id: &str,
    tick: &GoalPlanCoordinationTick,
    factory: &mut dyn PlanStepDispatchFactory,
    dispatcher: &dyn TurnDispatcher,
    policies: CoordinatorPolicies<'_>,
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
        GoalPlanDecision::ProposePlan
        | GoalPlanDecision::ProposeReplacement { .. }
        | GoalPlanDecision::NeedsOperator { .. } => {
            Ok(GoalPlanAdvanceOutcome::AwaitingPolicy(decision))
        }
        GoalPlanDecision::ResolveFailedPlan {
            plan_id,
            plan_revision,
            failed_step_ids,
        } => {
            let Some(policy) = policies.failure else {
                return Ok(GoalPlanAdvanceOutcome::AwaitingPolicy(decision));
            };
            let plans = reconstruct_plan_graph(ledger, session_id).map_err(|_| {
                GoalPlanCoordinatorError::Coordination(GoalPlanCoordinationError::CorruptState)
            })?;
            let plan = plans
                .values()
                .find(|plan| {
                    let definition = plan.snapshot.definition();
                    definition.plan_id().as_str() == plan_id
                        && definition.plan_revision() == *plan_revision
                        && plan.session_version == snapshot.session_version
                        && plan.through_position == snapshot.through_position
                })
                .ok_or(GoalPlanCoordinatorError::Coordination(
                    GoalPlanCoordinationError::ConcurrentModification,
                ))?;
            let plan_definition_digest = plan.snapshot.definition().digest().map_err(|_| {
                GoalPlanCoordinatorError::Coordination(GoalPlanCoordinationError::CorruptState)
            })?;
            let suspension_reference = policy_suspension_reference(
                ledger,
                session_id,
                plan_id,
                *plan_revision,
                plan.snapshot.state(),
                snapshot.through_position,
            )?;
            let input = PlanFailureInput {
                session_version: snapshot.session_version,
                through_position: snapshot.through_position,
                goal_id: snapshot.goal_id.clone(),
                goal_revision: snapshot.goal_revision,
                goal_definition_digest: snapshot.goal_definition_digest.clone(),
                plan_id: plan_id.clone(),
                plan_revision: *plan_revision,
                plan_definition_digest,
                failed_step_ids: failed_step_ids.clone(),
                suspension_reference,
            };
            let planned = match policy.decide(&input) {
                PlanFailureDecision::Suspend {
                    policy_reference,
                    reason,
                } if input.suspension_reference.is_none() => {
                    validate_failure_policy_metadata(&policy_reference, Some(&reason))?;
                    let evidence = failure_policy_evidence(
                        &input,
                        &policy_reference,
                        "suspend",
                        Some(&reason),
                    )?;
                    let continuation_reference =
                        format!("{policy_reference}:{}", evidence.sha256());
                    let mut planned = plan_suspend_failed_plan(
                        ledger,
                        session_id,
                        plan,
                        plan.state_version,
                        &plan_context(&snapshot, tick, "suspend-failed-plan"),
                        continuation_reference.clone(),
                    )
                    .map_err(GoalPlanCoordinatorError::Plan)?;
                    let mut goal = crate::plan_goal_transition(
                        ledger,
                        session_id,
                        goal_id,
                        snapshot.goal_revision,
                        &goal_context(&snapshot, tick, "suspend-failed-goal"),
                        GoalRuntimeTransition::Suspend {
                            reason,
                            suspension_reference: Some(continuation_reference),
                        },
                    )
                    .map_err(GoalPlanCoordinatorError::Goal)?;
                    planned.facts.append(&mut goal.facts);
                    planned
                }
                PlanFailureDecision::Resume { policy_reference } => {
                    validate_failure_policy_metadata(&policy_reference, None)?;
                    let Some(continuation_reference) = input.suspension_reference.as_deref() else {
                        return Ok(GoalPlanAdvanceOutcome::AwaitingPolicy(decision));
                    };
                    if continuation_policy_reference(continuation_reference)
                        != Some(policy_reference.as_str())
                    {
                        return Err(GoalPlanCoordinatorError::InvalidTick);
                    }
                    let mut planned = plan_resume_failed_plan(
                        ledger,
                        session_id,
                        plan,
                        plan.state_version,
                        &plan_context(&snapshot, tick, "resume-failed-plan"),
                        continuation_reference.into(),
                    )
                    .map_err(GoalPlanCoordinatorError::Plan)?;
                    let plan_reference = crate::goal_plan_coordination::canonical_plan_reference(
                        plan.snapshot.definition(),
                    )
                    .map_err(GoalPlanCoordinatorError::Coordination)?;
                    let mut goal = crate::plan_goal_transition(
                        ledger,
                        session_id,
                        goal_id,
                        snapshot.goal_revision,
                        &goal_context(&snapshot, tick, "resume-failed-goal"),
                        GoalRuntimeTransition::Activate {
                            plan_reference: Some(plan_reference),
                        },
                    )
                    .map_err(GoalPlanCoordinatorError::Goal)?;
                    planned.facts.append(&mut goal.facts);
                    planned
                }
                PlanFailureDecision::Fail {
                    policy_reference,
                    reason,
                } if input.suspension_reference.is_none() => {
                    validate_failure_policy_metadata(&policy_reference, Some(&reason))?;
                    let evidence =
                        failure_policy_evidence(&input, &policy_reference, "fail", Some(&reason))?;
                    crate::plan_fail_plan(
                        ledger,
                        session_id,
                        plan,
                        plan.state_version,
                        &plan_context(&snapshot, tick, "fail-plan-policy"),
                        reason,
                        Some(evidence),
                    )
                    .map_err(GoalPlanCoordinatorError::Plan)?
                }
                PlanFailureDecision::Replan { policy_reference }
                    if input.suspension_reference.is_none() =>
                {
                    validate_failure_policy_metadata(&policy_reference, None)?;
                    let evidence =
                        failure_policy_evidence(&input, &policy_reference, "replan", None)?;
                    let command_id = coordination_command_id(&snapshot, "admit-replan");
                    let admission = plan_replan_admission_fact(
                        &command_id,
                        tick,
                        &input,
                        &policy_reference,
                        evidence,
                    )?;
                    let admission_fact_id = admission.fact_id.as_str().to_owned();
                    let planned = crate::PlannedPlanCommand {
                        facts: vec![admission],
                        next: plan.clone(),
                    };
                    commit_plan_command(
                        ledger,
                        session_id.clone(),
                        snapshot.session_version,
                        &planned,
                    )
                    .map_err(GoalPlanCoordinatorError::Plan)?;
                    return Ok(GoalPlanAdvanceOutcome::ReplanRequested {
                        decision,
                        admission_fact_id,
                        policy_reference,
                    });
                }
                PlanFailureDecision::Suspend { .. }
                | PlanFailureDecision::Replan { .. }
                | PlanFailureDecision::Fail { .. }
                | PlanFailureDecision::Defer { .. } => {
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
        GoalPlanDecision::AdmitProposedPlan {
            plan_id,
            plan_revision,
        } => {
            let Some(policy) = policies.admission else {
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
            let sources = plans
                .values()
                .filter(|plan| {
                    let definition = plan.snapshot.definition();
                    matches!(
                        plan.snapshot.state(),
                        PlanState::Adopted | PlanState::Running | PlanState::Suspended
                    ) && definition.plan_id().as_str() == plan_id
                        && definition.plan_revision().checked_add(1) == Some(*plan_revision)
                })
                .collect::<Vec<_>>();
            if sources.len() > 1 {
                return Err(GoalPlanCoordinatorError::Coordination(
                    GoalPlanCoordinationError::CorruptState,
                ));
            }
            let source = sources.first().copied();
            let policy_decision = policy.decide(&PlanAdmissionInput {
                session_version: snapshot.session_version,
                through_position: snapshot.through_position,
                goal_id: snapshot.goal_id.clone(),
                goal_revision: snapshot.goal_revision,
                goal_definition_digest: snapshot.goal_definition_digest.clone(),
                plan_id: plan_id.clone(),
                plan_revision: *plan_revision,
                plan_definition_digest,
                prior_plan_revision: source
                    .map(|value| value.snapshot.definition().plan_revision()),
            });
            let context = plan_context(&snapshot, tick, "admit-plan");
            let planned = match policy_decision {
                PlanAdmissionDecision::Adopt { policy_reference } => {
                    if let Some(source) = source {
                        let verified =
                            verify_plan_carry_forward(ledger, session_id, source, proposal)
                                .map_err(GoalPlanCoordinatorError::Plan)?;
                        let replacement = crate::plan_plan_replacement(
                            source,
                            proposal,
                            &verified,
                            &context,
                            &policy_reference,
                        )
                        .map_err(GoalPlanCoordinatorError::Plan)?;
                        commit_plan_replacement(
                            ledger,
                            session_id.clone(),
                            snapshot.session_version,
                            &replacement,
                        )
                        .map_err(GoalPlanCoordinatorError::Plan)?;
                        return Ok(GoalPlanAdvanceOutcome::Committed(decision));
                    }
                    crate::plan_adopt_plan(
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
                    .map_err(GoalPlanCoordinatorError::Plan)?
                }
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

fn valid_policy_reference(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-' | b'/')
        })
}

fn valid_safe_code(value: &str) -> bool {
    let mut bytes = value.bytes();
    (1..=64).contains(&value.len())
        && bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_failure_policy_metadata(
    policy_reference: &str,
    reason: Option<&str>,
) -> Result<(), GoalPlanCoordinatorError> {
    if !valid_policy_reference(policy_reference)
        || reason.is_some_and(|value| !valid_safe_code(value))
    {
        Err(GoalPlanCoordinatorError::InvalidTick)
    } else {
        Ok(())
    }
}

fn failure_policy_evidence(
    input: &PlanFailureInput,
    policy_reference: &str,
    decision: &str,
    reason: Option<&str>,
) -> Result<CanonicalPayload, GoalPlanCoordinatorError> {
    CanonicalPayload::from_value(&serde_json::json!({
        "contract":"garive.plan-failure-policy-decision",
        "version":1,
        "decision":decision,
        "policy_reference":policy_reference,
        "reason":reason,
        "input":{
            "session_version":input.session_version,
            "through_position":input.through_position,
            "goal_id":input.goal_id,
            "goal_revision":input.goal_revision,
            "goal_definition_digest":input.goal_definition_digest,
            "plan_id":input.plan_id,
            "plan_revision":input.plan_revision,
            "plan_definition_digest":input.plan_definition_digest,
            "failed_step_ids":input.failed_step_ids,
            "suspension_reference":input.suspension_reference,
        },
    }))
    .map_err(|_| GoalPlanCoordinatorError::InvalidTick)
}

fn plan_replan_admission_fact(
    command_id: &str,
    tick: &GoalPlanCoordinationTick,
    input: &PlanFailureInput,
    policy_reference: &str,
    evidence: CanonicalPayload,
) -> Result<FactDraft, GoalPlanCoordinatorError> {
    let payload = CanonicalPayload::from_value(&serde_json::json!({
        "command_id":command_id,
        "source_plan_id":input.plan_id,
        "source_plan_revision":input.plan_revision,
        "source_plan_definition_digest":input.plan_definition_digest,
        "goal_id":input.goal_id,
        "goal_revision":input.goal_revision,
        "goal_definition_digest":input.goal_definition_digest,
        "failed_step_ids":input.failed_step_ids,
        "policy_reference":policy_reference,
        "expected_session_version":input.session_version,
        "through_position":input.through_position,
        "decision_evidence":{"digest":evidence.sha256(),"inline_utf8":evidence.as_json()},
    }))
    .map_err(|_| GoalPlanCoordinatorError::InvalidTick)?;
    Ok(FactDraft {
        fact_id: FactId::try_from(command_id).map_err(|_| GoalPlanCoordinatorError::InvalidTick)?,
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("plan.replan.admitted")
            .map_err(|_| GoalPlanCoordinatorError::InvalidTick)?,
        schema_version: 1,
        payload,
        recorded_at: tick.recorded_at.clone(),
    })
}

fn policy_suspension_reference(
    ledger: &SqliteLedger,
    session_id: &SessionId,
    plan_id: &str,
    plan_revision: u64,
    state: PlanState,
    through_position: u64,
) -> Result<Option<String>, GoalPlanCoordinatorError> {
    if state == PlanState::Running {
        return Ok(None);
    }
    if state != PlanState::Suspended {
        return Err(GoalPlanCoordinatorError::Coordination(
            GoalPlanCoordinationError::ConcurrentModification,
        ));
    }
    let facts = ledger
        .read_facts(session_id, 0, through_position, None)
        .map_err(|_| {
            GoalPlanCoordinatorError::Coordination(GoalPlanCoordinationError::CorruptState)
        })?;
    let candidates = facts
        .iter()
        .filter(|fact| fact.kind.as_str() == "plan.suspended")
        .filter_map(|fact| {
            let value = serde_json::from_str::<Value>(fact.payload.as_json()).ok()?;
            (value.get("plan_id")?.as_str()? == plan_id
                && value.get("plan_revision")?.as_u64()? == plan_revision)
                .then_some(value)
        })
        .collect::<Vec<_>>();
    let value = candidates
        .last()
        .ok_or(GoalPlanCoordinatorError::Coordination(
            GoalPlanCoordinationError::CorruptState,
        ))?;
    if value.get("continuation_kind").and_then(Value::as_str) != Some("policy") {
        return Err(GoalPlanCoordinatorError::Coordination(
            GoalPlanCoordinationError::ContinuationUnavailable,
        ));
    }
    let reference = value
        .get("continuation_reference")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinatorError::Coordination(
            GoalPlanCoordinationError::CorruptState,
        ))?;
    continuation_policy_reference(reference).ok_or(GoalPlanCoordinatorError::Coordination(
        GoalPlanCoordinationError::CorruptState,
    ))?;
    Ok(Some(reference.into()))
}

fn continuation_policy_reference(value: &str) -> Option<&str> {
    let (reference, digest) = value.rsplit_once(':')?;
    (valid_policy_reference(reference)
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    .then_some(reference)
}

fn pending_replan_admission(
    facts: &[DurableFact],
    goal: &crate::GoalRuntimeState,
    plan_id: &str,
    plan_revision: u64,
    failed_step_ids: &[String],
) -> Result<Option<String>, GoalPlanCoordinationError> {
    let Some((admission_index, fact)) = facts
        .iter()
        .enumerate()
        .rev()
        .find(|(_, fact)| fact.kind.as_str() == "plan.replan.admitted")
    else {
        return Ok(None);
    };
    let value = serde_json::from_str::<Value>(fact.payload.as_json())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let stored_failed = value
        .get("failed_step_ids")
        .and_then(Value::as_array)
        .ok_or(GoalPlanCoordinationError::CorruptState)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(GoalPlanCoordinationError::CorruptState)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let goal_digest = goal
        .snapshot
        .definition()
        .digest()
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    if value.get("source_plan_id").and_then(Value::as_str) != Some(plan_id)
        || value.get("source_plan_revision").and_then(Value::as_u64) != Some(plan_revision)
        || value.get("goal_id").and_then(Value::as_str)
            != Some(goal.snapshot.definition().goal_id().as_str())
        || value.get("goal_revision").and_then(Value::as_u64) != Some(goal.snapshot.revision())
        || value.get("goal_definition_digest").and_then(Value::as_str) != Some(goal_digest.as_str())
        || value
            .get("expected_session_version")
            .and_then(Value::as_u64)
            .and_then(|version| version.checked_add(1))
            .is_none_or(|version| version > goal.session_version)
        || value
            .get("through_position")
            .and_then(Value::as_u64)
            .and_then(|position| position.checked_add(1))
            != Some(fact.position)
        || stored_failed != failed_step_ids
        || !valid_pending_replan_tail(&facts[admission_index + 1..], fact, &value)?
    {
        return Err(GoalPlanCoordinationError::CorruptState);
    }
    Ok(Some(fact.fact_id.as_str().into()))
}

fn valid_pending_replan_tail(
    tail: &[DurableFact],
    admission: &DurableFact,
    admission_value: &Value,
) -> Result<bool, GoalPlanCoordinationError> {
    if tail.is_empty() {
        return Ok(true);
    }
    let request = &tail[0];
    if request.kind.as_str() != "plan.replan.proposal.requested"
        || admission.position.checked_add(1) != Some(request.position)
    {
        return Ok(false);
    }
    let request_value = serde_json::from_str::<Value>(request.payload.as_json())
        .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
    let turn_id = request_value
        .get("turn_id")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let execution_id = request_value
        .get("execution_id")
        .and_then(Value::as_str)
        .ok_or(GoalPlanCoordinationError::CorruptState)?;
    let same_source = request_value
        .get("admission_fact_id")
        .and_then(Value::as_str)
        == Some(admission.fact_id.as_str())
        && request_value.get("source_plan_id").and_then(Value::as_str)
            == admission_value
                .get("source_plan_id")
                .and_then(Value::as_str)
        && request_value
            .get("source_plan_revision")
            .and_then(Value::as_u64)
            == admission_value
                .get("source_plan_revision")
                .and_then(Value::as_u64)
        && request_value
            .get("source_plan_definition_digest")
            .and_then(Value::as_str)
            == admission_value
                .get("source_plan_definition_digest")
                .and_then(Value::as_str)
        && request_value
            .get("expected_session_version")
            .and_then(Value::as_u64)
            == admission_value
                .get("expected_session_version")
                .and_then(Value::as_u64)
                .and_then(|value| value.checked_add(1))
        && request_value
            .get("through_position")
            .and_then(Value::as_u64)
            == Some(admission.position);
    if !same_source {
        return Ok(false);
    }
    for (index, fact) in tail.iter().enumerate().skip(1) {
        if fact.kind.as_str() == "plan.replan.proposal.result_bound" {
            if index + 1 != tail.len() {
                return Ok(false);
            }
            let value = serde_json::from_str::<Value>(fact.payload.as_json())
                .map_err(|_| GoalPlanCoordinationError::CorruptState)?;
            if value.get("admission_fact_id").and_then(Value::as_str)
                != Some(admission.fact_id.as_str())
                || value.get("request_fact_id").and_then(Value::as_str)
                    != Some(request.fact_id.as_str())
                || value.get("planner_turn_id").and_then(Value::as_str) != Some(turn_id)
                || value.get("planner_execution_id").and_then(Value::as_str) != Some(execution_id)
            {
                return Ok(false);
            }
        } else if fact.turn_id.as_ref().map(|value| value.as_str()) != Some(turn_id)
            && fact.execution_id.as_ref().map(|value| value.as_str()) != Some(execution_id)
        {
            return Ok(false);
        }
    }
    Ok(true)
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
    let proposed = plans
        .iter()
        .copied()
        .filter(|plan| plan.snapshot.state() == PlanState::Proposed)
        .collect::<Vec<_>>();
    if let Some(plan) = authoritative.first().copied() {
        if proposed.len() > 1 {
            return Ok(GoalPlanDecision::NeedsOperator {
                reason: "ambiguous_plan_proposals",
            });
        }
        if let Some(target) = proposed.first().copied() {
            let source = plan.snapshot.definition();
            let target = target.snapshot.definition();
            if target.plan_id() != source.plan_id()
                || source.plan_revision().checked_add(1) != Some(target.plan_revision())
                || target.goal_revision() != goal.snapshot.revision()
            {
                return Err(GoalPlanCoordinationError::CorruptState);
            }
            return Ok(GoalPlanDecision::AdmitProposedPlan {
                plan_id: target.plan_id().as_str().into(),
                plan_revision: target.plan_revision(),
            });
        }
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
        return Ok(if plan.snapshot.state() == PlanState::Suspended {
            failed_plan_decision(plan).unwrap_or(GoalPlanDecision::NoAction)
        } else {
            GoalPlanDecision::NoAction
        });
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
    Ok(failed_plan_decision(plan).unwrap_or(GoalPlanDecision::NoAction))
}

fn failed_plan_decision(plan: &crate::PlanRuntimeState) -> Option<GoalPlanDecision> {
    let failed_step_ids = plan
        .snapshot
        .definition()
        .steps()
        .iter()
        .filter(|step| {
            plan.snapshot
                .step(step.step_id())
                .map(|value| value.state())
                == Some(StepState::Failed)
        })
        .map(|step| step.step_id().as_str().to_owned())
        .collect::<Vec<_>>();
    (!failed_step_ids.is_empty()).then(|| GoalPlanDecision::ResolveFailedPlan {
        plan_id: plan.snapshot.definition().plan_id().as_str().into(),
        plan_revision: plan.snapshot.definition().plan_revision(),
        failed_step_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::{valid_policy_reference, valid_safe_code};

    #[test]
    fn failure_policy_metadata_is_bounded_and_content_free() {
        assert!(valid_policy_reference("runtime:failure-policy/v1"));
        assert!(!valid_policy_reference("contains secret"));
        assert!(!valid_policy_reference(&"x".repeat(129)));
        assert!(valid_safe_code("attempts_exhausted"));
        assert!(!valid_safe_code("Human diagnostic"));
        assert!(!valid_safe_code(&"x".repeat(65)));
    }
}
