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
    commit_plan_command, commit_plan_replacement, get_turn, plan_plan_replacement,
    plan_plan_transition, plan_propose_plan, plan_start_step_execution, plan_start_turn,
    reconstruct_plan, verify_plan_carry_forward, EffectiveRuntimeLimits, GetTurnQuery,
    PlanCommandContext, PlanRetryPosture, PlanRuntimeError, PlanRuntimeState,
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
    let first_failure = fail_step(
        &state,
        "retry-attempt-1",
        &first_execution,
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
    assert_eq!(
        fail_step(
            &state,
            "retry-attempt-2",
            &second_execution,
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

#[test]
fn replacement_atomically_supersedes_and_reconstructs_verified_carry_forward() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("carry-forward.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let proposed = plan_propose_plan(&context("carry-propose-1"), definition_revision(1)).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 1, &proposed).unwrap();
    let mut source = recover_revision(&ledger, &session, 1);
    let adopted = plan_plan_transition(
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
    let target_proposal =
        plan_propose_plan(&context("carry-propose-2"), definition_revision(2)).unwrap();
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
        Err(PlanRuntimeError::Invalid)
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
    definition_revision(1)
}

fn definition_revision(revision: u64) -> PlanDefinitionV1 {
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
