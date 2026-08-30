use std::collections::BTreeSet;

use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanState, PlanStepId,
    PlanStepV1, StepState,
};
use garive_runtime::{
    commit_plan_command, plan_plan_transition, plan_propose_plan, reconstruct_plan,
    PlanCommandContext, PlanRuntimeError, PlanRuntimeState, PlanRuntimeTransition, SqliteLedger,
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
