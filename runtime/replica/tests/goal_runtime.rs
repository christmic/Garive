use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalId, GoalScopeV1, GoalState,
};
use garive_ledger::{CanonicalPayload, CommitDisposition, FactDraft, FactId, FactKind, SessionId};
use garive_runtime::{
    commit_goal_command, plan_create_goal, plan_goal_transition, reconstruct_goal,
    GoalCommandContext, GoalRuntimeError, GoalRuntimeTransition, SqliteLedger,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn goal_commands_reconstruct_across_restart_and_exact_replay() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("goal.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let resume;
    {
        let mut ledger = SqliteLedger::open(&path).unwrap();
        open_session(&mut ledger, &session);
        let created = plan_create_goal(&context("create"), definition()).unwrap();
        assert!(
            garive_ledger::validate_runtime_fact(&created.facts[0]).is_ok(),
            "{}",
            created.facts[0].payload.as_json()
        );
        commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();

        let draft = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
        let active = plan_goal_transition(
            &draft,
            1,
            &context("activate"),
            GoalRuntimeTransition::Activate {
                plan_reference: Some("plan-1@1".into()),
            },
        )
        .unwrap();
        commit_goal_command(&mut ledger, session.clone(), 2, &active).unwrap();

        let active = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
        let suspended = plan_goal_transition(
            &active,
            2,
            &context("suspend"),
            GoalRuntimeTransition::Suspend {
                reason: "approval_required".into(),
                suspension_reference: Some("interaction-1".into()),
            },
        )
        .unwrap();
        commit_goal_command(&mut ledger, session.clone(), 3, &suspended).unwrap();
        resume = plan_goal_transition(
            &suspended.next,
            3,
            &context("resume"),
            GoalRuntimeTransition::Activate {
                plan_reference: Some("plan-1@1".into()),
            },
        )
        .unwrap();
    }

    let mut reopened = SqliteLedger::open(&path).unwrap();
    let suspended = reconstruct_goal(&reopened, &session, "goal-1").unwrap();
    assert_eq!(suspended.snapshot.revision(), 3);
    assert_eq!(suspended.snapshot.state(), GoalState::Suspended);
    assert_eq!(suspended.attempt_number, 1);
    let committed = commit_goal_command(&mut reopened, session.clone(), 4, &resume).unwrap();
    assert_eq!(committed.disposition, CommitDisposition::Committed);
    let replayed = commit_goal_command(&mut reopened, session.clone(), 0, &resume).unwrap();
    assert_eq!(replayed.disposition, CommitDisposition::Replayed);
    let recovered = reconstruct_goal(&reopened, &session, "goal-1").unwrap();
    assert_eq!(
        (recovered.snapshot.revision(), recovered.attempt_number),
        (4, 1)
    );
    assert_eq!(recovered.snapshot.state(), GoalState::Active);
}

#[test]
fn stale_session_writer_and_changed_command_replay_fail_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("race.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut first = SqliteLedger::open(&path).unwrap();
    open_session(&mut first, &session);
    let created = plan_create_goal(&context("create"), definition()).unwrap();
    commit_goal_command(&mut first, session.clone(), 1, &created).unwrap();
    let state = reconstruct_goal(&first, &session, "goal-1").unwrap();
    let mut second = SqliteLedger::open(&path).unwrap();
    let winner = plan_goal_transition(
        &state,
        1,
        &context("winner"),
        GoalRuntimeTransition::Activate {
            plan_reference: None,
        },
    )
    .unwrap();
    let loser = plan_goal_transition(
        &state,
        1,
        &context("loser"),
        GoalRuntimeTransition::Cancel {
            reason: "user_request".into(),
        },
    )
    .unwrap();
    commit_goal_command(&mut first, session.clone(), 2, &winner).unwrap();
    assert_eq!(
        commit_goal_command(&mut second, session.clone(), 2, &loser),
        Err(GoalRuntimeError::RevisionConflict)
    );

    let changed = plan_create_goal(
        &context("create"),
        GoalDefinitionV1::new(
            GoalId::new("goal-1").unwrap(),
            "Changed objective",
            criteria(),
            scope(),
            bounds(),
            None,
            capabilities(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        commit_goal_command(&mut first, session, 0, &changed),
        Err(GoalRuntimeError::CommandConflict)
    );
}

fn open_session(ledger: &mut SqliteLedger, session: &SessionId) {
    let fact = FactDraft {
        fact_id: FactId::try_from("session-open").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({})).unwrap(),
        recorded_at: timestamp().into(),
    };
    ledger.commit(session.clone(), 0, vec![fact]).unwrap();
}

fn definition() -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new("goal-1").unwrap(),
        "Ship the durable slice",
        criteria(),
        scope(),
        bounds(),
        None,
        capabilities(),
    )
    .unwrap()
}

fn criteria() -> Vec<GoalCriterion> {
    vec![GoalCriterion::UserAcceptance {
        criterion_id: GoalCriterionId::new("accepted").unwrap(),
        response_schema_digest: "a".repeat(64),
    }]
}

fn scope() -> GoalScopeV1 {
    GoalScopeV1::new(Some("session-1".into()), ["workspace-1".into()]).unwrap()
}

fn bounds() -> GoalBoundsV1 {
    GoalBoundsV1::new(2, 3, 2, Some(10_000), Some(60_000)).unwrap()
}

fn capabilities() -> [GoalCapabilityReference; 1] {
    [GoalCapabilityReference::new("tools", "catalogue-v1").unwrap()]
}

fn context(command_id: &str) -> GoalCommandContext {
    GoalCommandContext {
        command_id: command_id.into(),
        actor_reference: "user:fixture".into(),
        recorded_at: timestamp().into(),
    }
}

fn timestamp() -> &'static str {
    "2026-08-31T00:00:00Z"
}
