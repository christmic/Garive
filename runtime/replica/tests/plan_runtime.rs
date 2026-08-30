use std::collections::BTreeSet;

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, FactDraft,
    FactId, FactKind, SessionId,
};
use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanState, PlanStepId,
    PlanStepV1, StepState,
};
use garive_runtime::{
    commit_plan_command, get_turn, plan_plan_transition, plan_propose_plan,
    plan_start_step_execution, plan_start_turn, reconstruct_plan, EffectiveRuntimeLimits,
    GetTurnQuery, PlanCommandContext, PlanRetryPosture, PlanRuntimeError, PlanRuntimeState,
    PlanRuntimeTransition, PlanStepExecutionStart, RuntimeCommandId, SqliteLedger,
    StartTurnCommand,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn claims_expire_before_start_and_started_work_recovers_to_completion() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("plan.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(&context("propose"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let mut state = recover(&ledger, &session);

    let adopted = plan_plan_transition(
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
        plan_plan_transition(
            &state,
            3,
            &context("late-start"),
            start_request("claim-1", 1, 20, "attempt-late", "execution-late"),
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
    let started = plan_plan_transition(
        &state,
        state.state_version,
        &context("start-prepare"),
        start_request("claim-2", 2, 25, "attempt-1", "execution-1"),
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
    let completed = complete_step(
        &state,
        "prepare",
        "attempt-1",
        "execution-1",
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
    let deliver_start = plan_plan_transition(
        &state,
        state.state_version,
        &context("start-deliver"),
        PlanRuntimeTransition::Start {
            step_id: step_id("deliver"),
            claim_id: "claim-3".into(),
            lease_epoch: 1,
            clock_revision: "monotonic-v1".into(),
            observed_at_tick: 35,
            attempt_id: "attempt-2".into(),
            execution_id: "execution-2".into(),
            execution_snapshot_digest: digest('e'),
            sandbox_profile_digest: digest('f'),
            safety_decision_id: "safety-decision-2".into(),
        },
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
    let deliver_complete = complete_step(
        &state,
        "deliver",
        "attempt-2",
        "execution-2",
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
    let terminal = plan_plan_transition(
        &state,
        state.state_version,
        &context("complete-plan"),
        PlanRuntimeTransition::CompletePlan {
            reduction_evidence: evidence(),
        },
    )
    .unwrap();
    commit_plan_command(
        &mut ledger,
        session.clone(),
        state.session_version,
        &terminal,
    )
    .unwrap();
    assert_eq!(
        recover(&ledger, &session).snapshot.state(),
        PlanState::Completed
    );
}

#[test]
fn competing_sqlite_claims_have_one_winner_and_exact_command_replay() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("claim-race.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut first = SqliteLedger::open(&path).unwrap();
    open_session(&mut first, &session);
    let proposed = plan_propose_plan(&context("race-propose"), definition()).unwrap();
    commit_plan_command(&mut first, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&first, &session);
    let adopted = plan_plan_transition(
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
    commit_plan_command(&mut first, session.clone(), state.session_version, &winner).unwrap();
    assert_eq!(
        commit_plan_command(&mut second, session.clone(), state.session_version, &loser),
        Err(PlanRuntimeError::RevisionConflict)
    );
    assert!(commit_plan_command(&mut first, session, 0, &winner).is_ok());
}

#[test]
fn retry_posture_reopens_within_bounds_and_refuses_exhaustion_after_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("retry.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(&context("retry-propose"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&ledger, &session);
    let adopted = plan_plan_transition(
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
    let first_start = plan_plan_transition(
        &state,
        state.state_version,
        &context("retry-start-first"),
        start_request(
            "retry-claim-1",
            1,
            15,
            "retry-attempt-1",
            "retry-execution-1",
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
    let first_failure = fail_step(
        &state,
        "retry-attempt-1",
        "retry-execution-1",
        "retry-fail-first",
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
    let second_start = plan_plan_transition(
        &state,
        state.state_version,
        &context("retry-start-second"),
        start_request(
            "retry-claim-2",
            2,
            25,
            "retry-attempt-2",
            "retry-execution-2",
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
    assert_eq!(
        fail_step(
            &state,
            "retry-attempt-2",
            "retry-execution-2",
            "retry-fail-second",
        ),
        Err(PlanRuntimeError::BoundExceeded)
    );
}

#[test]
fn step_start_and_c6_execution_commit_as_one_restart_safe_command() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("atomic-start.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(&context("atomic-propose"), definition()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let draft = recover(&ledger, &session);
    let adopted = plan_plan_transition(
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
        &start_turn(&session, "atomic-start", state.through_position),
        state.through_position,
    )
    .unwrap();
    let execution_id = turn.execution_id.clone().unwrap();
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

    let ledger = SqliteLedger::open(&path).unwrap();
    let recovered = recover(&ledger, &session);
    assert_eq!(
        recovered.active_claims[&step_id("prepare")]
            .execution_id
            .as_deref(),
        Some(execution_id.as_str())
    );
    let turn = get_turn(
        &ledger,
        &GetTurnQuery {
            session_id: session,
            turn_id: turn.turn_id,
            through_position: None,
        },
    )
    .unwrap();
    assert_eq!(turn.execution_id.as_ref(), Some(&execution_id));
}

fn recover(ledger: &SqliteLedger, session: &SessionId) -> PlanRuntimeState {
    reconstruct_plan(
        ledger,
        session,
        "plan-1",
        1,
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

fn start_request(
    claim: &str,
    epoch: u64,
    observed: u64,
    attempt: &str,
    execution: &str,
) -> PlanRuntimeTransition {
    PlanRuntimeTransition::Start {
        step_id: step_id("prepare"),
        claim_id: claim.into(),
        lease_epoch: epoch,
        clock_revision: "monotonic-v1".into(),
        observed_at_tick: observed,
        attempt_id: attempt.into(),
        execution_id: execution.into(),
        execution_snapshot_digest: digest('e'),
        sandbox_profile_digest: digest('f'),
        safety_decision_id: "safety-decision-1".into(),
    }
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

fn fail_step(
    state: &PlanRuntimeState,
    attempt: &str,
    execution: &str,
    command: &str,
) -> Result<garive_runtime::PlannedPlanCommand, PlanRuntimeError> {
    plan_plan_transition(
        state,
        state.state_version,
        &context(command),
        PlanRuntimeTransition::FailStep {
            step_id: step_id("prepare"),
            attempt_id: attempt.into(),
            execution_id: execution.into(),
            reason: "verification_failed".into(),
            evidence: Some(evidence()),
            retry_posture: PlanRetryPosture::Retry,
        },
    )
}

fn definition() -> PlanDefinitionV1 {
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
        1,
        "goal-1",
        2,
        digest('a'),
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
    ledger
        .commit(
            session.clone(),
            0,
            vec![FactDraft {
                fact_id: FactId::try_from("session-open").unwrap(),
                turn_id: None,
                execution_id: None,
                model_request_id: None,
                tool_invocation_id: None,
                kind: FactKind::new("session.opened").unwrap(),
                schema_version: 1,
                payload: CanonicalPayload::from_value(&json!({})).unwrap(),
                recorded_at: timestamp().into(),
            }],
        )
        .unwrap();
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
fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
fn timestamp() -> &'static str {
    "2026-08-31T00:00:00Z"
}

fn start_turn(session: &SessionId, command: &str, _: u64) -> StartTurnCommand {
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
