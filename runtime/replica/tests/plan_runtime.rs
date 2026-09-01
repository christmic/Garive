use std::{collections::BTreeSet, sync::Arc};

use garive_core::{
    AgentFailureReason, AgentOutcome, ExecutionReport, GovernedSuspensionBinding,
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, SuspensionReason,
    TerminalRecoveryAction, UsageSummary,
};
use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalEvidenceId, GoalEvidenceKind, GoalEvidenceV1, GoalId, GoalScopeV1, GoalState,
};
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, FactDraft,
    FactId, FactKind, SessionId, ToolInvocationId,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelOutputSettings, ModelPort, ModelPortFailure, ModelRequest, ModelStopReason, ModelUsage,
    TextMode, TokenCount, UsageSource,
};
use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanState, PlanStepId,
    PlanStepV1, StepState,
};
use garive_runtime::{
    commit_goal_command, commit_plan_command, commit_plan_replacement, commit_planned_turn,
    get_turn, plan_activate_goal_from_authoritative_plan, plan_adopt_plan,
    plan_complete_owned_step_from_turn, plan_complete_plan, plan_continue_owned_plan_turn,
    plan_continue_turn, plan_core_terminal, plan_fail_owned_step_from_turn, plan_goal_transition,
    plan_next_turn_cancellation_for_goal, plan_plan_replacement, plan_plan_transition,
    plan_propose_plan, plan_resume_goal_from_continued_turn, plan_start_step_execution,
    plan_start_turn, plan_succeed_goal_from_completed_plan, plan_suspend_goal_from_owned_turn,
    plan_suspend_owned_plan_from_turn, reconstruct_execution_work_binding, reconstruct_goal,
    reconstruct_plan, reconstruct_plan_graph, reconstruct_suspended_turn,
    verify_plan_carry_forward, ContinuationInput, ContinueTurnCommand, CoreTerminalContext,
    EffectiveRuntimeLimits, GetTurnQuery, GoalCommandContext, GoalPlanCoordinationError,
    GoalRuntimeError, GoalRuntimeTransition, InteractionInputRepresentation, LocalExecutionAttempt,
    LocalExecutionPolicy, LocalExecutionWorker, LocalWorkerDisposition, PlanCommandContext,
    PlanRuntimeError, PlanRuntimeState, PlanRuntimeTransition, PlanStepExecutionStart,
    RuntimeCommandId, SqliteLedger, StartTurnCommand,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[test]
fn claims_expire_before_start_and_started_work_recovers_to_completion() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("plan.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(&ledger, &session, &context("propose"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let mut state = recover(&ledger, &session);

    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &state,
        1,
        &context("adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "plan-policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &adopted,
    )
    .unwrap();
    state = recover(&ledger, &session);
    assert_eq!(state.snapshot.state(), PlanState::Adopted);
    assert_eq!(state.snapshot.ready_steps(), vec![&step_id("prepare")]);

    let first_claim = claim(&state, "claim-1", 1, 10, 20, "claim-first");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &first_claim,
    )
    .unwrap();
    state = recover(&ledger, &session);
    assert_eq!(state.state_version, 3);
    assert_eq!(
        start_step(
            &state,
            &session,
            StepStartFixture::new("prepare", "claim-1", 1, 20, "attempt-late", "late-start",),
        ),
        Err(PlanRuntimeError::ClaimStale)
    );
    let expired = plan_plan_transition(
        &state,
        3,
        &context("expire"),
        PlanRuntimeTransition::ExpireClaim {
            step_id: step_id("prepare"),
            claim_id: "claim-1".into(),
            lease_epoch: 1,
            clock_revision: "monotonic-v1".into(),
            observed_at_tick: 20,
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &expired,
    )
    .unwrap();
    drop(ledger);

    let mut ledger = SqliteLedger::open(&path).unwrap();
    state = recover(&ledger, &session);
    assert_eq!(
        state.snapshot.step(&step_id("prepare")).unwrap().state(),
        StepState::Ready
    );
    let second_claim = claim(&state, "claim-2", 2, 21, 30, "claim-second");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &second_claim,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let started = start_step(
        &state,
        &session,
        StepStartFixture::new("prepare", "claim-2", 2, 25, "attempt-1", "start-prepare"),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &started,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let prepare_execution = active_execution(&state, "prepare");
    let completed = complete_step(
        &state,
        "prepare",
        "attempt-1",
        &prepare_execution,
        "complete-prepare",
    );
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &completed,
    )
    .unwrap();
    state = recover(&ledger, &session);
    assert_eq!(state.snapshot.ready_steps(), vec![&step_id("deliver")]);

    let deliver_claim = claim_step(&state, "deliver", "claim-3", 1, 31, 40, "claim-deliver");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &deliver_claim,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let deliver_start = start_step(
        &state,
        &session,
        StepStartFixture::new("deliver", "claim-3", 1, 35, "attempt-2", "start-deliver"),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &deliver_start,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let deliver_execution = active_execution(&state, "deliver");
    let deliver_complete = complete_step(
        &state,
        "deliver",
        "attempt-2",
        &deliver_execution,
        "complete-deliver",
    );
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &deliver_complete,
    )
    .unwrap();
    state = recover(&ledger, &session);
    assert_eq!(
        plan_plan_transition(
            &state,
            state.state_version,
            &context("bypass-complete-plan"),
            PlanRuntimeTransition::CompletePlan {
                reduction_evidence: evidence(),
            },
        ),
        Err(PlanRuntimeError::TransitionInvalid)
    );
    let acceptance = acceptance_payload();
    assert_eq!(
        plan_complete_plan(
            &ledger,
            &session,
            &state,
            state.state_version,
            &context("incomplete-plan"),
            Vec::new(),
        ),
        Err(PlanRuntimeError::EvidenceInvalid)
    );
    let observed_version = state.session_version;
    let terminal = plan_complete_plan(
        &ledger,
        &session,
        &state,
        state.state_version,
        &context("complete-plan"),
        vec![
            GoalEvidenceV1::new(
                GoalEvidenceId::new("accepted-evidence").unwrap(),
                GoalCriterionId::new("accepted").unwrap(),
                GoalEvidenceKind::DurableFact,
                "session-open",
                acceptance.sha256(),
                observed_version,
            )
            .unwrap(),
            GoalEvidenceV1::new(
                GoalEvidenceId::new("artifact-evidence").unwrap(),
                GoalCriterionId::new("artifact").unwrap(),
                GoalEvidenceKind::DurableFact,
                "goal-activate",
                goal_activation_payload().sha256(),
                observed_version,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &terminal,
    )
    .unwrap();
    let completed = recover(&ledger, &session);
    assert_eq!(completed.snapshot.state(), PlanState::Completed);
    let graph = reconstruct_plan_graph(&ledger, &session).unwrap();
    let projected = graph.get(&("plan-1".into(), 1)).unwrap();
    assert_eq!(graph.len(), 1);
    assert_eq!(projected.snapshot.state(), PlanState::Completed);
    assert_eq!(projected.state_version, 11);

    let goal_terminal = plan_succeed_goal_from_completed_plan(
        &ledger,
        &session,
        "goal-1",
        completed.session_version,
        2,
        &goal_context("succeed-goal"),
    )
    .unwrap();
    commit_goal_command(
        &mut ledger,
        session.clone(),
        completed.session_version,
        &goal_terminal,
    )
    .unwrap();
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-1")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Succeeded
    );

    let stored: String = ledger
        .connection_for_test()
        .query_row(
            "SELECT payload_json FROM ledger_facts WHERE fact_id = 'complete-plan'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut completion: serde_json::Value = serde_json::from_str(&stored).unwrap();
    let empty = evidence();
    completion["reduction_evidence"] = json!({
        "digest":empty.sha256(),
        "inline_utf8":empty.as_json(),
    });
    let corrupted = CanonicalPayload::from_value(&completion).unwrap();
    ledger
        .connection_for_test()
        .execute(
            "UPDATE ledger_facts SET payload_json = ?1, payload_sha256 = ?2 \
             WHERE fact_id = 'complete-plan'",
            rusqlite::params![corrupted.as_json(), corrupted.sha256()],
        )
        .unwrap();
    assert_eq!(
        reconstruct_plan(
            &ledger,
            &session,
            "plan-1",
            1,
            &criteria(),
            &BTreeSet::new(),
            &capabilities(),
        ),
        Err(PlanRuntimeError::RecoveryCorrupt)
    );
}

#[test]
fn competing_sqlite_claims_have_one_winner_and_exact_command_replay() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("claim-race.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut first = SqliteLedger::open(&path).unwrap();
    open_session(&mut first, &session);
    let proposed =
        plan_propose_plan(&first, &session, &context("race-propose"), definition()).unwrap();
    commit_plan_command(&mut first, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&first, &session);
    let adopted = plan_adopt_plan(
        &first,
        &session,
        &draft,
        1,
        &context("race-adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(&mut first, session.clone(), draft.session_version, &adopted).unwrap();
    let state = recover(&first, &session);
    let winner = claim(&state, "claim-a", 1, 10, 20, "race-winner");
    let loser = claim(&state, "claim-b", 1, 10, 20, "race-loser");
    let mut second = SqliteLedger::open(&path).unwrap();
    let winner_commit =
        commit_plan_command(&mut first, session.clone(), state.session_version, &winner).unwrap();
    assert_eq!(
        commit_plan_command(&mut second, session.clone(), state.session_version, &loser),
        Err(PlanRuntimeError::RevisionConflict)
    );
    first
        .commit(
            session.clone(),
            winner_commit.session_version,
            vec![session_fact(
                "race-goal-cancel",
                "goal.cancelled",
                json!({
                    "command_id":"race-goal-cancel",
                    "goal_id":"goal-1",
                    "revision":3,
                    "reason":"user_request",
                    "actor_reference":"user:fixture",
                }),
            )],
        )
        .unwrap();
    assert!(commit_plan_command(&mut first, session, 0, &winner).is_ok());
}

#[test]
fn proposal_and_adoption_require_the_current_durable_goal_binding() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("goal-binding.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    assert_eq!(
        plan_propose_plan(
            &ledger,
            &session,
            &context("wrong-binding"),
            definition_with_goal_digest(1, &digest('f')),
        ),
        Err(PlanRuntimeError::BindingStale)
    );

    let proposed =
        plan_propose_plan(&ledger, &session, &context("bound-proposal"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let replay =
        plan_propose_plan(&ledger, &session, &context("bound-proposal"), definition()).unwrap();
    assert_eq!(
        commit_plan_command(&mut ledger, session.clone(), 0, &replay)
            .unwrap()
            .disposition,
        garive_ledger::CommitDisposition::Replayed
    );

    let draft = recover(&ledger, &session);
    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &draft,
        draft.state_version,
        &context("bound-adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "plan-policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        draft.session_version,
        &adopted,
    )
    .unwrap();

    ledger
        .commit(
            session.clone(),
            3,
            vec![session_fact(
                "goal-cancel",
                "goal.cancelled",
                json!({
                    "command_id":"goal-cancel",
                    "goal_id":"goal-1",
                    "revision":3,
                    "reason":"user_request",
                    "actor_reference":"user:fixture",
                }),
            )],
        )
        .unwrap();
    let state = recover(&ledger, &session);
    assert_eq!(
        plan_adopt_plan(
            &ledger,
            &session,
            &state,
            state.state_version,
            &context("adopt-terminal-goal"),
            PlanRuntimeTransition::Adopt {
                expected_goal_revision: 2,
                expected_prior_plan_revision: None,
                policy_reference: "plan-policy-v1".into(),
                carry_forward_evidence: evidence(),
            },
        ),
        Err(PlanRuntimeError::BindingStale)
    );
    let post_cancel_claim = claim(
        &state,
        "post-cancel-claim",
        1,
        10,
        20,
        "claim-terminal-goal",
    );
    assert_eq!(
        commit_plan_command(
            &mut ledger,
            session.clone(),
            state.session_version,
            &post_cancel_claim,
        ),
        Err(PlanRuntimeError::BindingStale)
    );
    assert!(ledger
        .read_facts(&session, 0, u64::MAX, None)
        .unwrap()
        .iter()
        .all(|fact| fact.fact_id.as_str() != "claim-terminal-goal"));
}

#[tokio::test]
async fn completed_owned_turn_reduces_to_step_with_observed_evidence_after_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("turn-step-reduction.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(
        &ledger,
        &session,
        &context("reduce-propose"),
        single_step_definition(),
    )
    .unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&ledger, &session);
    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &draft,
        draft.state_version,
        &context("reduce-adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "plan-policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        draft.session_version,
        &adopted,
    )
    .unwrap();
    let adopted = recover(&ledger, &session);
    let claimed = claim(&adopted, "reduce-claim", 1, 10, 20, "reduce-claim-command");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        adopted.session_version,
        &claimed,
    )
    .unwrap();
    let claimed = recover(&ledger, &session);
    let turn = plan_start_turn(
        &start_turn(&session, "reduce-start"),
        claimed.through_position,
    )
    .unwrap();
    let turn_id = turn.turn_id.clone();
    let execution_id = turn.execution_id.clone().unwrap();
    let started = plan_start_step_execution(
        &claimed,
        claimed.state_version,
        &context("reduce-start"),
        PlanStepExecutionStart {
            step_id: step_id("prepare"),
            claim_id: "reduce-claim".into(),
            lease_epoch: 1,
            clock_revision: "monotonic-v1".into(),
            observed_at_tick: 15,
            attempt_id: "reduce-attempt".into(),
            sandbox_profile_digest: digest('f'),
            safety_decision_id: "safety-reduce".into(),
        },
        &turn,
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        claimed.session_version,
        &started,
    )
    .unwrap();
    let running = recover(&ledger, &session);
    let committed = garive_runtime::CommittedTurn {
        session_id: session.clone(),
        turn_id: turn_id.clone(),
        execution_id: execution_id.clone(),
        definition_id: "agent-definition-1".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: digest('b'),
        session_version: running.session_version,
        committed_position: running.through_position,
    };
    assert_eq!(
        plan_complete_owned_step_from_turn(
            &ledger,
            &session,
            "goal-1",
            &turn_id,
            running.session_version,
            2,
            &context("reduce-before-terminal"),
        ),
        Err(GoalPlanCoordinationError::CompletedTurnUnavailable)
    );
    drop(ledger);
    let worker =
        LocalExecutionWorker::new(&path, worker_policy(), Arc::new(PlanCompletingModel)).unwrap();
    let disposition = worker.execute(&committed, &worker_attempt()).await.unwrap();
    let LocalWorkerDisposition::TerminalCommitted { positions } = disposition else {
        panic!("first dispatch must commit terminal and Step reduction")
    };
    assert_eq!(positions.len(), 5);
    assert_eq!(
        worker.execute(&committed, &worker_attempt()).await.unwrap(),
        LocalWorkerDisposition::AlreadyTerminal
    );
    let ledger = SqliteLedger::open(&path).unwrap();
    let completed = ledger
        .read_facts(&session, 0, u64::MAX, None)
        .unwrap()
        .into_iter()
        .find(|fact| fact.kind.as_str() == "plan.step.completed")
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(completed.payload.as_json()).unwrap();
    let criterion_evidence: serde_json::Value = serde_json::from_str(
        payload["criterion_evidence"]["inline_utf8"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["step_id"], "prepare");
    assert_eq!(payload["attempt_id"], "reduce-attempt");
    assert_eq!(criterion_evidence[0]["criterion_id"], "accepted");
    assert_eq!(criterion_evidence[1]["criterion_id"], "artifact");
    drop(ledger);
    let ledger = SqliteLedger::open(&path).unwrap();
    let recovered = recover(&ledger, &session);
    assert_eq!(
        recovered
            .snapshot
            .step(&step_id("prepare"))
            .unwrap()
            .state(),
        StepState::Completed
    );
    assert_eq!(recovered.snapshot.state(), PlanState::Completed);
    assert!(recovered.snapshot.ready_steps().is_empty());
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-1")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Succeeded
    );
}

#[tokio::test]
async fn retry_posture_reopens_within_bounds_and_refuses_exhaustion_after_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("retry.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed =
        plan_propose_plan(&ledger, &session, &context("retry-propose"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&ledger, &session);
    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &draft,
        draft.state_version,
        &context("retry-adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        draft.session_version,
        &adopted,
    )
    .unwrap();

    let mut state = recover(&ledger, &session);
    let first_claim = claim(&state, "retry-claim-1", 1, 10, 20, "retry-claim-first");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &first_claim,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let first_start = start_step(
        &state,
        &session,
        StepStartFixture::new(
            "prepare",
            "retry-claim-1",
            1,
            15,
            "retry-attempt-1",
            "retry-start-first",
        ),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &first_start,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let first_execution = active_execution(&state, "prepare");
    let first_turn = commit_failed_execution(
        &mut ledger,
        &session,
        &state,
        &first_execution,
        AgentFailureReason::PortFailure,
    );
    state = recover(&ledger, &session);
    let first_failure = plan_fail_owned_step_from_turn(
        &ledger,
        &session,
        "goal-1",
        &first_turn,
        state.session_version,
        2,
        &context("retry-fail-first"),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &first_failure,
    )
    .unwrap();
    drop(ledger);

    let mut ledger = SqliteLedger::open(&path).unwrap();
    state = recover(&ledger, &session);
    assert_eq!(
        state.snapshot.step(&step_id("prepare")).unwrap().state(),
        StepState::Ready
    );
    let second_claim = claim(&state, "retry-claim-2", 2, 21, 30, "retry-claim-second");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &second_claim,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let second_start = start_step(
        &state,
        &session,
        StepStartFixture::new(
            "prepare",
            "retry-claim-2",
            2,
            25,
            "retry-attempt-2",
            "retry-start-second",
        ),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &second_start,
    )
    .unwrap();
    state = recover(&ledger, &session);
    let second_execution = active_execution(&state, "prepare");
    let execution_id = garive_ledger::ExecutionId::try_from(second_execution.as_str()).unwrap();
    let turn_id = ledger
        .read_facts(&session, 0, state.through_position, None)
        .unwrap()
        .into_iter()
        .find(|fact| {
            fact.kind.as_str() == "execution.started"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .unwrap()
        .turn_id
        .unwrap();
    assert_eq!(
        plan_plan_transition(
            &state,
            state.state_version,
            &context("bypass-plan-failure"),
            PlanRuntimeTransition::FailPlan {
                reason: "attempts_exhausted".into(),
                evidence: None,
            },
        ),
        Err(PlanRuntimeError::TransitionInvalid)
    );
    let committed = garive_runtime::CommittedTurn {
        session_id: session.clone(),
        turn_id,
        execution_id,
        definition_id: "agent-definition-1".into(),
        definition_revision: "revision-1".into(),
        snapshot_digest: digest('b'),
        session_version: state.session_version,
        committed_position: state.through_position,
    };
    drop(ledger);
    let worker =
        LocalExecutionWorker::new(&path, worker_policy(), Arc::new(PlanFailingModel)).unwrap();
    let LocalWorkerDisposition::TerminalCommitted { positions } =
        worker.execute(&committed, &worker_attempt()).await.unwrap()
    else {
        panic!("failed final attempt must close Step, Plan and Goal")
    };
    assert_eq!(positions.len(), 5);
    assert_eq!(
        worker.execute(&committed, &worker_attempt()).await.unwrap(),
        LocalWorkerDisposition::AlreadyTerminal
    );
    let ledger = SqliteLedger::open(&path).unwrap();
    assert_eq!(
        recover(&ledger, &session).snapshot.state(),
        PlanState::Failed
    );
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-1")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Failed
    );
}

#[test]
fn step_start_and_c6_execution_commit_as_one_restart_safe_command() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("atomic-start.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed =
        plan_propose_plan(&ledger, &session, &context("atomic-propose"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&ledger, &session);
    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &draft,
        draft.state_version,
        &context("atomic-adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        draft.session_version,
        &adopted,
    )
    .unwrap();
    let state = recover(&ledger, &session);
    let claimed = claim(&state, "atomic-claim", 1, 10, 20, "atomic-claim-command");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &claimed,
    )
    .unwrap();
    let state = recover(&ledger, &session);

    let turn = plan_start_turn(
        &start_turn(&session, "atomic-start"),
        state.through_position,
    )
    .unwrap();
    let execution_id = turn.execution_id.clone().unwrap();
    assert_eq!(
        plan_plan_transition(
            &state,
            state.state_version,
            &context("atomic-start"),
            PlanRuntimeTransition::Start {
                step_id: step_id("prepare"),
                claim_id: "atomic-claim".into(),
                lease_epoch: 1,
                clock_revision: "monotonic-v1".into(),
                observed_at_tick: 15,
                attempt_id: "atomic-attempt".into(),
                execution_id: execution_id.as_str().into(),
                execution_snapshot_digest: digest('b'),
                sandbox_profile_digest: digest('f'),
                safety_decision_id: "safety-decision-atomic".into(),
            },
        ),
        Err(PlanRuntimeError::TransitionInvalid)
    );
    let started = plan_start_step_execution(
        &state,
        state.state_version,
        &context("atomic-start"),
        PlanStepExecutionStart {
            step_id: step_id("prepare"),
            claim_id: "atomic-claim".into(),
            lease_epoch: 1,
            clock_revision: "monotonic-v1".into(),
            observed_at_tick: 15,
            attempt_id: "atomic-attempt".into(),
            sandbox_profile_digest: digest('f'),
            safety_decision_id: "safety-decision-atomic".into(),
        },
        &turn,
    )
    .unwrap();
    assert_eq!(
        started
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        [
            "turn.started",
            "turn.input",
            "execution.started",
            "plan.step.started"
        ]
    );
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &started,
    )
    .unwrap();
    drop(ledger);

    let mut ledger = SqliteLedger::open(&path).unwrap();
    let recovered = recover(&ledger, &session);
    assert_eq!(
        recovered.active_claims[&step_id("prepare")]
            .execution_id
            .as_deref(),
        Some(execution_id.as_str())
    );
    let turn_id = turn.turn_id.clone();
    assert_eq!(
        plan_suspend_goal_from_owned_turn(
            &ledger,
            &session,
            "goal-1",
            recovered.session_version,
            2,
            &goal_context("suspend-open-turn"),
        ),
        Err(GoalPlanCoordinationError::AuthoritativePlanUnavailable)
    );
    ledger
        .commit(
            session.clone(),
            recovered.session_version,
            interaction_request(&turn_id, &execution_id),
        )
        .unwrap();
    let report = ExecutionReport {
        outcome: AgentOutcome::Suspended {
            reason: SuspensionReason::ExternalInputRequired,
            partial_items: vec![],
            last_durable_position: recovered.through_position,
            governed_binding: Some(GovernedSuspensionBinding::Interaction {
                suspension_id: "suspension-external-input".into(),
                interaction_id: "interaction-external-input".into(),
                invocation_id: "tool-external-input".into(),
                prepared_digest: digest('d'),
            }),
        },
        completed_iterations: 1,
        usage: UsageSummary {
            input_tokens: TokenCount::Known(1),
            output_tokens: TokenCount::Known(1),
            estimated: false,
        },
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn_id.clone(),
            execution_id: execution_id.clone(),
            recorded_at: timestamp().into(),
        },
        &report,
    )
    .unwrap();
    ledger
        .commit(session.clone(), recovered.session_version + 1, terminal)
        .unwrap();
    let running = recover(&ledger, &session);
    let plan_suspension = plan_suspend_owned_plan_from_turn(
        &ledger,
        &session,
        "goal-1",
        running.session_version,
        2,
        &context("suspend-plan-from-turn"),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        running.session_version,
        &plan_suspension,
    )
    .unwrap();
    let plan_suspended = recover(&ledger, &session);
    assert_eq!(plan_suspended.snapshot.state(), PlanState::Suspended);
    assert_eq!(
        plan_suspended
            .snapshot
            .step(&step_id("prepare"))
            .unwrap()
            .state(),
        StepState::Suspended
    );
    let active = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
    let suspension = plan_suspend_goal_from_owned_turn(
        &ledger,
        &session,
        "goal-1",
        active.session_version,
        2,
        &goal_context("suspend-goal-from-turn"),
    )
    .unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(suspension.facts[0].payload.as_json()).unwrap();
    assert_eq!(payload["reason"], "external_input_required");
    assert_eq!(payload["suspension_reference"], "suspension-external-input");
    commit_goal_command(
        &mut ledger,
        session.clone(),
        active.session_version,
        &suspension,
    )
    .unwrap();
    let suspended = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
    assert_eq!(suspended.snapshot.state(), GoalState::Suspended);
    assert_eq!(
        plan_suspend_goal_from_owned_turn(
            &ledger,
            &session,
            "goal-1",
            suspended.session_version,
            3,
            &goal_context("duplicate-goal-suspension"),
        ),
        Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid
        ))
    );
    assert_eq!(
        plan_activate_goal_from_authoritative_plan(
            &ledger,
            &session,
            "goal-1",
            suspended.session_version,
            3,
            &goal_context("bypass-turn-continuation"),
        ),
        Err(GoalPlanCoordinationError::Goal(
            GoalRuntimeError::TransitionInvalid
        ))
    );
    let suspended_turn = reconstruct_suspended_turn(&ledger.load_turn(&turn_id).unwrap()).unwrap();
    let plan_awaiting_resume = recover(&ledger, &session);
    let continuation = plan_continue_turn(
        &ContinueTurnCommand {
            command_id: RuntimeCommandId::new("continue-goal-owned-turn").unwrap(),
            session_id: session.clone(),
            turn_id: turn_id.clone(),
            expected_suspension_id: suspended_turn.suspension_id.clone(),
            expected_session_version: suspended_turn.session_version,
            continuation_input: ContinuationInput::InteractionResponse {
                canonical_json: r#""provided""#.into(),
                representation: InteractionInputRepresentation::StringField,
            },
            interaction: suspended_turn.interaction.clone(),
            recorded_at: "2026-08-31T00:00:01Z".into(),
        },
        &suspended_turn,
    )
    .unwrap();
    let continued_execution_id = continuation.execution_id.clone().unwrap();
    let plan_continuation = plan_continue_owned_plan_turn(
        &ledger,
        &session,
        "goal-1",
        plan_awaiting_resume.session_version,
        &PlanCommandContext {
            command_id: "continue-goal-owned-turn".into(),
            actor_reference: "runtime:goal-plan-coordinator".into(),
            recorded_at: "2026-08-31T00:00:01Z".into(),
        },
        &continuation,
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        suspended_turn.session_version,
        &plan_continuation,
    )
    .unwrap();
    let awaiting_resume = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
    let resume = plan_resume_goal_from_continued_turn(
        &ledger,
        &session,
        "goal-1",
        awaiting_resume.session_version,
        3,
        &goal_context("resume-goal-from-turn-continuation"),
    )
    .unwrap();
    commit_goal_command(
        &mut ledger,
        session.clone(),
        awaiting_resume.session_version,
        &resume,
    )
    .unwrap();
    let active = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
    assert_eq!(active.snapshot.state(), GoalState::Active);
    assert_eq!(active.snapshot.revision(), 4);
    let cancellation = plan_goal_transition(
        &ledger,
        &session,
        "goal-1",
        4,
        &goal_context("cancel-started-goal"),
        GoalRuntimeTransition::Cancel {
            reason: "user_request".into(),
        },
    )
    .unwrap();
    commit_goal_command(
        &mut ledger,
        session.clone(),
        active.session_version,
        &cancellation,
    )
    .unwrap();
    let propagation =
        plan_next_turn_cancellation_for_goal(&ledger, &session, "goal-1", "2026-08-31T00:00:01Z")
            .unwrap()
            .unwrap();
    assert_eq!(propagation.turn_id, turn_id);
    commit_planned_turn(
        &mut ledger,
        session.clone(),
        propagation.expected_session_version,
        &propagation.planned,
    )
    .unwrap();
    let turn = get_turn(
        &ledger,
        &GetTurnQuery {
            session_id: session.clone(),
            turn_id: turn_id.clone(),
            through_position: None,
        },
    )
    .unwrap();
    assert_eq!(turn.execution_id.as_ref(), Some(&continued_execution_id));
    assert!(turn.cancellation_requested);
    assert!(plan_next_turn_cancellation_for_goal(
        &ledger,
        &session,
        "goal-1",
        "2026-08-31T00:00:02Z",
    )
    .unwrap()
    .is_none());
    let binding = reconstruct_execution_work_binding(&ledger, &turn.session_id, &execution_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(binding.goal_reference()).unwrap(),
        json!({
            "definition_digest":goal_definition().digest().unwrap(),
            "goal_id":"goal-1",
            "revision":2,
        })
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(binding.plan_reference()).unwrap(),
        json!({
            "definition_digest":definition().digest().unwrap(),
            "plan_id":"plan-1",
            "revision":1,
        })
    );
    drop(ledger);
    let ledger = SqliteLedger::open(&path).unwrap();
    assert!(plan_next_turn_cancellation_for_goal(
        &ledger,
        &session,
        "goal-1",
        "2026-08-31T00:00:03Z",
    )
    .unwrap()
    .is_none());
}

#[test]
fn replacement_atomically_supersedes_and_reconstructs_verified_carry_forward() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("carry-forward.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(
        &ledger,
        &session,
        &context("carry-propose-1"),
        definition_revision(1),
    )
    .unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let mut source = recover_revision(&ledger, &session, 1);
    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &source,
        source.state_version,
        &context("carry-adopt-1"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 2,
            expected_prior_plan_revision: None,
            policy_reference: "policy-v1".into(),
            carry_forward_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        source.session_version,
        &adopted,
    )
    .unwrap();
    source = recover_revision(&ledger, &session, 1);
    let claimed = claim(&source, "carry-claim", 1, 10, 20, "carry-claim-command");
    commit_plan_command(
        &mut ledger,
        session.clone(),
        source.session_version,
        &claimed,
    )
    .unwrap();
    source = recover_revision(&ledger, &session, 1);
    let started = start_step(
        &source,
        &session,
        StepStartFixture::new(
            "prepare",
            "carry-claim",
            1,
            15,
            "carry-attempt",
            "carry-start",
        ),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        source.session_version,
        &started,
    )
    .unwrap();
    source = recover_revision(&ledger, &session, 1);
    let completed = complete_step(
        &source,
        "prepare",
        "carry-attempt",
        &active_execution(&source, "prepare"),
        "carry-complete",
    );
    commit_plan_command(
        &mut ledger,
        session.clone(),
        source.session_version,
        &completed,
    )
    .unwrap();
    source = recover_revision(&ledger, &session, 1);
    let target_proposal = plan_propose_plan(
        &ledger,
        &session,
        &context("carry-propose-2"),
        definition_revision(2),
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        source.session_version,
        &target_proposal,
    )
    .unwrap();
    source = recover_revision(&ledger, &session, 1);
    let target = recover_revision(&ledger, &session, 2);
    let verified = verify_plan_carry_forward(&ledger, &session, &source, &target).unwrap();
    assert_eq!(
        verified.carried_steps(),
        &BTreeSet::from([step_id("prepare")])
    );
    assert_eq!(
        plan_plan_transition(
            &target,
            target.state_version,
            &context("carry-bypass"),
            PlanRuntimeTransition::Adopt {
                expected_goal_revision: 2,
                expected_prior_plan_revision: Some(1),
                policy_reference: "policy-v1".into(),
                carry_forward_evidence: verified.evidence().clone(),
            },
        ),
        Err(PlanRuntimeError::TransitionInvalid)
    );
    let replacement = plan_plan_replacement(
        &source,
        &target,
        &verified,
        &context("carry-replace"),
        "policy-v1",
    )
    .unwrap();
    let stale_replacement = plan_plan_replacement(
        &source,
        &target,
        &verified,
        &context("carry-replace-stale"),
        "policy-v1",
    )
    .unwrap();
    assert_eq!(
        replacement
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        ["plan.superseded", "plan.adopted"]
    );
    commit_plan_replacement(
        &mut ledger,
        session.clone(),
        source.session_version,
        &replacement,
    )
    .unwrap();
    let mut competing = SqliteLedger::open(&path).unwrap();
    assert_eq!(
        commit_plan_replacement(
            &mut competing,
            session.clone(),
            source.session_version,
            &stale_replacement,
        ),
        Err(PlanRuntimeError::RevisionConflict)
    );
    let old = recover_revision(&ledger, &session, 1);
    let new = recover_revision(&ledger, &session, 2);
    assert_eq!(old.snapshot.state(), PlanState::Superseded);
    assert_eq!(new.snapshot.state(), PlanState::Running);
    assert_eq!(
        new.snapshot.step(&step_id("prepare")).unwrap().state(),
        StepState::Completed
    );
    assert_eq!(new.snapshot.ready_steps(), vec![&step_id("deliver")]);

    let stored: String = ledger
        .connection_for_test()
        .query_row(
            "SELECT payload_json FROM ledger_facts WHERE fact_id = 'carry-replace-target'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mut adoption: serde_json::Value = serde_json::from_str(&stored).unwrap();
    let mut evidence_json: serde_json::Value = serde_json::from_str(
        adoption["carry_forward_evidence"]["inline_utf8"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    evidence_json[0]["terminal_position"] = json!(999);
    let corrupted_evidence = CanonicalPayload::from_value(&evidence_json).unwrap();
    adoption["carry_forward_evidence"] = json!({
        "digest": corrupted_evidence.sha256(),
        "inline_utf8": corrupted_evidence.as_json(),
    });
    let corrupted_adoption = CanonicalPayload::from_value(&adoption).unwrap();
    ledger
        .connection_for_test()
        .execute(
            "UPDATE ledger_facts SET payload_json = ?1, payload_sha256 = ?2 \
             WHERE fact_id = 'carry-replace-target'",
            rusqlite::params![corrupted_adoption.as_json(), corrupted_adoption.sha256()],
        )
        .unwrap();
    assert_eq!(
        reconstruct_plan(
            &ledger,
            &session,
            "plan-1",
            2,
            &criteria(),
            &BTreeSet::new(),
            &capabilities(),
        ),
        Err(PlanRuntimeError::RecoveryCorrupt)
    );
}

fn recover(ledger: &SqliteLedger, session: &SessionId) -> PlanRuntimeState {
    recover_revision(ledger, session, 1)
}

fn recover_revision(ledger: &SqliteLedger, session: &SessionId, revision: u64) -> PlanRuntimeState {
    reconstruct_plan(
        ledger,
        session,
        "plan-1",
        revision,
        &criteria(),
        &BTreeSet::new(),
        &capabilities(),
    )
    .unwrap()
}

fn claim(
    state: &PlanRuntimeState,
    claim_id: &str,
    epoch: u64,
    claimed: u64,
    expires: u64,
    command: &str,
) -> garive_runtime::PlannedPlanCommand {
    claim_step(state, "prepare", claim_id, epoch, claimed, expires, command)
}

fn claim_step(
    state: &PlanRuntimeState,
    step: &str,
    claim_id: &str,
    epoch: u64,
    claimed: u64,
    expires: u64,
    command: &str,
) -> garive_runtime::PlannedPlanCommand {
    plan_plan_transition(
        state,
        state.state_version,
        &context(command),
        PlanRuntimeTransition::Claim {
            step_id: step_id(step),
            claim_id: claim_id.into(),
            worker_reference: "worker:fixture".into(),
            lease_epoch: epoch,
            clock_revision: "monotonic-v1".into(),
            claimed_at_tick: claimed,
            expires_at_tick: expires,
        },
    )
    .unwrap()
}

struct StepStartFixture<'a> {
    step: &'a str,
    claim: &'a str,
    epoch: u64,
    observed: u64,
    attempt: &'a str,
    command: &'a str,
}

impl<'a> StepStartFixture<'a> {
    fn new(
        step: &'a str,
        claim: &'a str,
        epoch: u64,
        observed: u64,
        attempt: &'a str,
        command: &'a str,
    ) -> Self {
        Self {
            step,
            claim,
            epoch,
            observed,
            attempt,
            command,
        }
    }
}

fn start_step(
    state: &PlanRuntimeState,
    session: &SessionId,
    request: StepStartFixture<'_>,
) -> Result<garive_runtime::PlannedPlanCommand, PlanRuntimeError> {
    let turn = plan_start_turn(
        &start_turn(session, request.command),
        state.through_position,
    )
    .unwrap();
    plan_start_step_execution(
        state,
        state.state_version,
        &context(request.command),
        PlanStepExecutionStart {
            step_id: step_id(request.step),
            claim_id: request.claim.into(),
            lease_epoch: request.epoch,
            clock_revision: "monotonic-v1".into(),
            observed_at_tick: request.observed,
            attempt_id: request.attempt.into(),
            sandbox_profile_digest: digest('f'),
            safety_decision_id: "safety-decision-1".into(),
        },
        &turn,
    )
}

fn active_execution(state: &PlanRuntimeState, step: &str) -> String {
    state.active_claims[&step_id(step)]
        .execution_id
        .clone()
        .unwrap()
}

fn complete_step(
    state: &PlanRuntimeState,
    step: &str,
    attempt: &str,
    execution: &str,
    command: &str,
) -> garive_runtime::PlannedPlanCommand {
    plan_plan_transition(
        state,
        state.state_version,
        &context(command),
        PlanRuntimeTransition::CompleteStep {
            step_id: step_id(step),
            attempt_id: attempt.into(),
            execution_id: execution.into(),
            result_digest: digest('9'),
            step_evidence: evidence(),
            criterion_evidence: evidence(),
        },
    )
    .unwrap()
}

fn commit_failed_execution(
    ledger: &mut SqliteLedger,
    session: &SessionId,
    state: &PlanRuntimeState,
    execution: &str,
    reason: AgentFailureReason,
) -> garive_ledger::TurnId {
    let execution_id = garive_ledger::ExecutionId::try_from(execution).unwrap();
    let turn_id = ledger
        .read_facts(session, 0, state.through_position, None)
        .unwrap()
        .into_iter()
        .find(|fact| {
            fact.kind.as_str() == "execution.started"
                && fact.execution_id.as_ref() == Some(&execution_id)
        })
        .unwrap()
        .turn_id
        .unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(0),
        estimated: false,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn_id.clone(),
            execution_id,
            recorded_at: timestamp().into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Failed { reason },
            completed_iterations: 1,
            usage,
        },
    )
    .unwrap();
    ledger
        .commit(session.clone(), state.session_version, terminal)
        .unwrap();
    turn_id
}

fn definition() -> PlanDefinitionV1 {
    definition_revision(1)
}

fn single_step_definition() -> PlanDefinitionV1 {
    let capability = PlanCapabilityReference::new("tools", "catalogue-v1").unwrap();
    PlanDefinitionV1::new(
        PlanId::new("plan-1").unwrap(),
        1,
        "goal-1",
        2,
        goal_definition().digest().unwrap(),
        digest('b'),
        digest('c'),
        "safety-v1",
        vec![PlanStepV1::new(
            step_id("prepare"),
            "Prepare and deliver",
            [],
            ["accepted".into(), "artifact".into()],
            [capability.clone()],
            [digest('d')],
            2,
        )
        .unwrap()],
        PlanBoundsV1::new(1, 1, 2, None, None).unwrap(),
        &criteria(),
        &BTreeSet::new(),
        &BTreeSet::from([capability]),
    )
    .unwrap()
}

fn definition_revision(revision: u64) -> PlanDefinitionV1 {
    definition_with_goal_digest(revision, &goal_definition().digest().unwrap())
}

fn definition_with_goal_digest(revision: u64, goal_digest: &str) -> PlanDefinitionV1 {
    let capability = PlanCapabilityReference::new("tools", "catalogue-v1").unwrap();
    let steps = vec![
        PlanStepV1::new(
            step_id("prepare"),
            "Prepare",
            [],
            ["accepted".into()],
            [capability.clone()],
            [digest('d')],
            2,
        )
        .unwrap(),
        PlanStepV1::new(
            step_id("deliver"),
            "Deliver",
            [step_id("prepare")],
            ["artifact".into()],
            [capability.clone()],
            [digest('d')],
            2,
        )
        .unwrap(),
    ];
    PlanDefinitionV1::new(
        PlanId::new("plan-1").unwrap(),
        revision,
        "goal-1",
        2,
        goal_digest,
        digest('b'),
        digest('c'),
        "safety-v1",
        steps,
        PlanBoundsV1::new(4, 2, 6, None, None).unwrap(),
        &criteria(),
        &BTreeSet::new(),
        &BTreeSet::from([capability]),
    )
    .unwrap()
}

fn open_session(ledger: &mut SqliteLedger, session: &SessionId) {
    let goal = goal_definition();
    let goal_json = goal.canonical_json().unwrap();
    ledger
        .commit(
            session.clone(),
            0,
            vec![
                session_fact(
                    "session-open",
                    "session.opened",
                    serde_json::from_str(acceptance_payload().as_json()).unwrap(),
                ),
                session_fact(
                    "goal-create",
                    "goal.created",
                    json!({
                        "command_id":"goal-create",
                        "goal_id":"goal-1",
                        "revision":1,
                        "definition_digest":goal.digest().unwrap(),
                        "definition":{
                            "digest":format!("{:x}", Sha256::digest(goal_json.as_bytes())),
                            "inline_utf8":goal_json,
                        },
                        "actor_reference":"user:fixture",
                    }),
                ),
                session_fact(
                    "goal-activate",
                    "goal.activated",
                    serde_json::from_str(goal_activation_payload().as_json()).unwrap(),
                ),
            ],
        )
        .unwrap();
}

fn goal_definition() -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new("goal-1").unwrap(),
        "Execute the durable Plan",
        vec![
            GoalCriterion::DurableFact {
                criterion_id: GoalCriterionId::new("accepted").unwrap(),
                fact_kind: "session.opened".into(),
                subject_digest: acceptance_payload().sha256().into(),
            },
            GoalCriterion::DurableFact {
                criterion_id: GoalCriterionId::new("artifact").unwrap(),
                fact_kind: "goal.activated".into(),
                subject_digest: goal_activation_payload().sha256().into(),
            },
        ],
        GoalScopeV1::new(Some("session-1".into()), []).unwrap(),
        GoalBoundsV1::new(2, 3, 2, None, None).unwrap(),
        None,
        [GoalCapabilityReference::new("tools", "catalogue-v1").unwrap()],
    )
    .unwrap()
}

fn acceptance_payload() -> CanonicalPayload {
    CanonicalPayload::from_value(&json!({
        "command_id":"session-open",
        "definition_id":"agent-definition-1",
        "definition_revision":"revision-1",
        "snapshot_digest":digest('b'),
        "agent_instance_id":"agent-instance-1",
    }))
    .unwrap()
}

fn goal_activation_payload() -> CanonicalPayload {
    CanonicalPayload::from_value(&json!({
        "command_id":"goal-activate",
        "goal_id":"goal-1",
        "revision":2,
        "attempt_number":1,
        "actor_reference":"agent:fixture",
    }))
    .unwrap()
}

fn session_fact(id: &str, kind: &str, payload: serde_json::Value) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: timestamp().into(),
    }
}

fn interaction_request(
    turn_id: &garive_ledger::TurnId,
    execution_id: &garive_ledger::ExecutionId,
) -> Vec<FactDraft> {
    let response_schema = CanonicalPayload::from_value(&json!({"type":"string"})).unwrap();
    let effect = FactDraft {
        fact_id: FactId::try_from("effect-prepared-external-input").unwrap(),
        turn_id: Some(turn_id.clone()),
        execution_id: Some(execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: Some(ToolInvocationId::try_from("tool-external-input").unwrap()),
        kind: FactKind::new("effect.prepared").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({
            "prepared_digest":digest('d'),
            "tool_name":"external_input",
            "tool_revision":"revision-1",
            "replay_class":"never_replay",
            "model_call_id":"model-call-external-input",
        }))
        .unwrap(),
        recorded_at: timestamp().into(),
    };
    let interaction = FactDraft {
        fact_id: FactId::try_from("interaction-request-external-input").unwrap(),
        turn_id: Some(turn_id.clone()),
        execution_id: Some(execution_id.clone()),
        model_request_id: None,
        tool_invocation_id: Some(ToolInvocationId::try_from("tool-external-input").unwrap()),
        kind: FactKind::new("interaction.requested").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({
            "interaction_id":"interaction-external-input",
            "suspension_id":"suspension-external-input",
            "prepared_digest":digest('d'),
            "kind":"external_input",
            "prompt":{"digest":format!("{:x}", Sha256::digest(b"Provide input")),"inline_utf8":"Provide input"},
            "response_schema":{"digest":response_schema.sha256(),"inline_utf8":response_schema.as_json()},
            "response_schema_digest":response_schema.sha256(),
            "expiry_code":"none",
        }))
        .unwrap(),
        recorded_at: timestamp().into(),
    };
    vec![effect, interaction]
}

fn evidence() -> CanonicalPayload {
    CanonicalPayload::from_value(&json!([])).unwrap()
}
fn step_id(value: &str) -> PlanStepId {
    PlanStepId::new(value).unwrap()
}
fn criteria() -> BTreeSet<String> {
    ["accepted".into(), "artifact".into()].into_iter().collect()
}
fn capabilities() -> BTreeSet<PlanCapabilityReference> {
    BTreeSet::from([PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()])
}
fn context(command_id: &str) -> PlanCommandContext {
    PlanCommandContext {
        command_id: command_id.into(),
        actor_reference: "user:fixture".into(),
        recorded_at: timestamp().into(),
    }
}
fn goal_context(command_id: &str) -> GoalCommandContext {
    GoalCommandContext {
        command_id: command_id.into(),
        actor_reference: "runtime:goal-plan-coordinator".into(),
        recorded_at: timestamp().into(),
    }
}
fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
fn timestamp() -> &'static str {
    "2026-08-31T00:00:00Z"
}

struct PlanCompletingModel;
impl ModelPort for PlanCompletingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async {
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "prepared".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(3),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

struct PlanFailingModel;
impl ModelPort for PlanFailingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async { Err(ModelPortFailure::RequiredPortFailure) })
    }
}

fn worker_policy() -> LocalExecutionPolicy {
    LocalExecutionPolicy {
        model_target_id: "target-plan".into(),
        deployment_id: "deployment-plan".into(),
        recovery_policy_revision: "recovery-v1".into(),
        required_capabilities: vec![ModelCapability::Text],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(2_048),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 0,
            output_limit: OutputLimitAction::Suspend,
            transport: TerminalRecoveryAction::Suspend,
            unavailable: TerminalRecoveryAction::Suspend,
            missing_usage: MissingUsagePolicy::Stop,
        },
        max_context_items: 8,
        max_context_utf8_bytes: 1_024,
        max_model_attempts: 1,
    }
}

fn worker_attempt() -> LocalExecutionAttempt {
    LocalExecutionAttempt {
        worker_owner_id: "worker-plan".into(),
        lease_token: "unpredictable-plan-token".into(),
        now_ms: 1_000,
        lease_duration_ms: 5_000,
        recorded_at: timestamp().into(),
    }
}

fn start_turn(session: &SessionId, command: &str) -> StartTurnCommand {
    StartTurnCommand {
        command_id: RuntimeCommandId::new(command).unwrap(),
        session_id: session.clone(),
        agent_instance_id: AgentInstanceId::try_from("agent-instance-1").unwrap(),
        definition_id: AgentDefinitionId::try_from("agent-definition-1").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision-1").unwrap(),
        snapshot_digest: digest('b'),
        trusted_input: "Execute the claimed Plan step".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(4_096),
            max_output_tokens: Some(2_048),
            deadline_budget_ms: Some(30_000),
        },
        recorded_at: timestamp().into(),
    }
}
