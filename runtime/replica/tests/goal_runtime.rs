use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalEvidenceId, GoalEvidenceKind, GoalEvidenceV1, GoalId, GoalScopeV1, GoalState,
};
use garive_ledger::{CanonicalPayload, CommitDisposition, FactDraft, FactId, FactKind, SessionId};
use garive_runtime::{
    commit_goal_command, plan_create_goal, plan_goal_transition, reconstruct_goal,
    GoalCommandContext, GoalRuntimeError, GoalRuntimeTransition, SqliteLedger,
};
use serde_json::json;
use sha2::{Digest, Sha256};
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
        let created =
            plan_create_goal(&ledger, &session, &context("create"), definition()).unwrap();
        assert!(
            garive_ledger::validate_runtime_fact(&created.facts[0]).is_ok(),
            "{}",
            created.facts[0].payload.as_json()
        );
        commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();

        let active = plan_goal_transition(
            &ledger,
            &session,
            "goal-1",
            1,
            &context("activate"),
            GoalRuntimeTransition::Activate {
                plan_reference: Some("plan-1@1".into()),
            },
        )
        .unwrap();
        commit_goal_command(&mut ledger, session.clone(), 2, &active).unwrap();

        let suspended = plan_goal_transition(
            &ledger,
            &session,
            "goal-1",
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
            &ledger,
            &session,
            "goal-1",
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
    let created = plan_create_goal(&first, &session, &context("create"), definition()).unwrap();
    commit_goal_command(&mut first, session.clone(), 1, &created).unwrap();
    let mut second = SqliteLedger::open(&path).unwrap();
    let winner = plan_goal_transition(
        &first,
        &session,
        "goal-1",
        1,
        &context("winner"),
        GoalRuntimeTransition::Activate {
            plan_reference: None,
        },
    )
    .unwrap();
    let loser = plan_goal_transition(
        &second,
        &session,
        "goal-1",
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
        &first,
        &session,
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

#[test]
fn child_creation_uses_the_fixed_ledger_graph_and_parent_limits() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("children.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let parent = plan_create_goal(&ledger, &session, &context("parent"), definition()).unwrap();
    commit_goal_command(
        &mut ledger,
        session.clone(),
        parent.next.session_version,
        &parent,
    )
    .unwrap();

    let first = plan_create_goal(
        &ledger,
        &session,
        &context("child-1"),
        child_definition("child-1", "goal-1", 1),
    )
    .unwrap();
    commit_goal_command(
        &mut ledger,
        session.clone(),
        first.next.session_version,
        &first,
    )
    .unwrap();
    let replay = plan_create_goal(
        &ledger,
        &session,
        &context("child-1"),
        child_definition("child-1", "goal-1", 1),
    )
    .unwrap();
    assert_eq!(
        commit_goal_command(&mut ledger, session.clone(), 0, &replay)
            .unwrap()
            .disposition,
        CommitDisposition::Replayed
    );
    assert_eq!(
        plan_create_goal(
            &ledger,
            &session,
            &context("duplicate-child"),
            child_definition("child-1", "goal-1", 1),
        ),
        Err(GoalRuntimeError::CommandConflict)
    );

    let second = plan_create_goal(
        &ledger,
        &session,
        &context("child-2"),
        child_definition("child-2", "goal-1", 1),
    )
    .unwrap();
    commit_goal_command(
        &mut ledger,
        session.clone(),
        second.next.session_version,
        &second,
    )
    .unwrap();
    assert_eq!(
        plan_create_goal(
            &ledger,
            &session,
            &context("child-3"),
            child_definition("child-3", "goal-1", 1),
        ),
        Err(GoalRuntimeError::ScopeExceeded)
    );
    assert_eq!(
        plan_create_goal(
            &ledger,
            &session,
            &context("orphan"),
            child_definition("orphan", "missing-parent", 1),
        ),
        Err(GoalRuntimeError::ScopeExceeded)
    );
    assert_eq!(
        plan_create_goal(
            &ledger,
            &session,
            &context("wider"),
            child_definition("wider", "goal-1", 4),
        ),
        Err(GoalRuntimeError::ScopeExceeded)
    );
    assert_eq!(
        plan_goal_transition(
            &ledger,
            &session,
            "child-1",
            1,
            &context("widen-child"),
            GoalRuntimeTransition::Revise {
                definition: Box::new(child_definition("child-1", "goal-1", 4)),
                replacement_reason: "expand_scope".into(),
            },
        ),
        Err(GoalRuntimeError::ScopeExceeded)
    );
    assert_eq!(
        plan_goal_transition(
            &ledger,
            &session,
            "goal-1",
            1,
            &context("cycle-root"),
            GoalRuntimeTransition::Revise {
                definition: Box::new(child_definition("goal-1", "child-1", 1)),
                replacement_reason: "invalid_reparent".into(),
            },
        ),
        Err(GoalRuntimeError::Cycle)
    );
}

#[test]
fn reconstruction_rejects_orphan_and_cyclic_goal_prefixes() {
    for (name, facts) in [
        (
            "orphan",
            vec![created_fact(
                "create-orphan",
                child_definition("orphan", "missing-parent", 1),
            )],
        ),
        (
            "cycle",
            vec![
                created_fact("create-a", child_definition("goal-a", "goal-b", 1)),
                created_fact("create-b", child_definition("goal-b", "goal-a", 1)),
            ],
        ),
    ] {
        let directory = tempdir().unwrap();
        let path = directory.path().join(format!("{name}.sqlite3"));
        let session = SessionId::try_from("session-1").unwrap();
        let mut ledger = SqliteLedger::open(&path).unwrap();
        open_session(&mut ledger, &session);
        ledger.commit(session.clone(), 1, facts).unwrap();
        assert_eq!(
            reconstruct_goal(
                &ledger,
                &session,
                if name == "orphan" { "orphan" } else { "goal-a" }
            ),
            Err(GoalRuntimeError::RecoveryCorrupt),
            "{name}"
        );
    }
}

#[test]
fn creation_rejects_foreign_session_scope_and_terminal_parent() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("authority.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);

    let foreign = GoalDefinitionV1::new(
        GoalId::new("foreign").unwrap(),
        "Foreign Session scope",
        criteria(),
        GoalScopeV1::new(Some("session-2".into()), ["workspace-1".into()]).unwrap(),
        bounds(),
        None,
        capabilities(),
    )
    .unwrap();
    assert_eq!(
        plan_create_goal(&ledger, &session, &context("foreign"), foreign),
        Err(GoalRuntimeError::ScopeExceeded)
    );

    let parent = plan_create_goal(&ledger, &session, &context("parent"), definition()).unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &parent).unwrap();
    let cancelled = plan_goal_transition(
        &ledger,
        &session,
        "goal-1",
        1,
        &context("cancel-parent"),
        GoalRuntimeTransition::Cancel {
            reason: "user_request".into(),
        },
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 2, &cancelled).unwrap();
    assert_eq!(
        plan_create_goal(
            &ledger,
            &session,
            &context("late-child"),
            child_definition("late-child", "goal-1", 1),
        ),
        Err(GoalRuntimeError::TransitionInvalid)
    );
}

#[test]
fn success_resolves_evidence_against_the_fixed_ledger_prefix() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("evidence.sqlite3");
    let session = SessionId::try_from("session-1").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    open_session(&mut ledger, &session);
    let subject_digest = CanonicalPayload::from_value(&json!({}))
        .unwrap()
        .sha256()
        .to_owned();
    let created = plan_create_goal(
        &ledger,
        &session,
        &context("create-evidence"),
        durable_fact_definition(&subject_digest),
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();
    let active = plan_goal_transition(
        &ledger,
        &session,
        "goal-evidence",
        1,
        &context("activate-evidence"),
        GoalRuntimeTransition::Activate {
            plan_reference: None,
        },
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 2, &active).unwrap();

    let evidence = GoalEvidenceV1::new(
        GoalEvidenceId::new("evidence-1").unwrap(),
        GoalCriterionId::new("durable").unwrap(),
        GoalEvidenceKind::DurableFact,
        "session-open",
        &subject_digest,
        3,
    )
    .unwrap();
    let stale = GoalEvidenceV1::new(
        GoalEvidenceId::new("evidence-stale").unwrap(),
        GoalCriterionId::new("durable").unwrap(),
        GoalEvidenceKind::DurableFact,
        "session-open",
        &subject_digest,
        2,
    )
    .unwrap();
    assert_eq!(
        plan_goal_transition(
            &ledger,
            &session,
            "goal-evidence",
            2,
            &context("stale-evidence"),
            GoalRuntimeTransition::Succeed {
                evidence: vec![stale],
            },
        ),
        Err(GoalRuntimeError::EvidenceInvalid)
    );
    let succeeded = plan_goal_transition(
        &ledger,
        &session,
        "goal-evidence",
        2,
        &context("succeed-evidence"),
        GoalRuntimeTransition::Succeed {
            evidence: vec![evidence],
        },
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 3, &succeeded).unwrap();
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-evidence")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Succeeded
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

fn durable_fact_definition(subject_digest: &str) -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new("goal-evidence").unwrap(),
        "Prove one exact durable fact",
        vec![GoalCriterion::DurableFact {
            criterion_id: GoalCriterionId::new("durable").unwrap(),
            fact_kind: "session.opened".into(),
            subject_digest: subject_digest.into(),
        }],
        scope(),
        bounds(),
        None,
        capabilities(),
    )
    .unwrap()
}

fn child_definition(goal_id: &str, parent_id: &str, max_attempts: u32) -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new(goal_id).unwrap(),
        "Complete a narrowed child objective",
        criteria(),
        GoalScopeV1::new(None, ["workspace-1".into()]).unwrap(),
        GoalBoundsV1::new(max_attempts, 2, 1, Some(5_000), Some(30_000)).unwrap(),
        Some(GoalId::new(parent_id).unwrap()),
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

fn created_fact(command_id: &str, definition: GoalDefinitionV1) -> FactDraft {
    let definition_json = definition.canonical_json().unwrap();
    FactDraft {
        fact_id: FactId::try_from(command_id).unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("goal.created").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({
            "command_id": command_id,
            "goal_id": definition.goal_id().as_str(),
            "revision": 1,
            "definition_digest": definition.digest().unwrap(),
            "definition": {
                "digest": format!("{:x}", Sha256::digest(definition_json.as_bytes())),
                "inline_utf8": definition_json,
            },
            "actor_reference": "user:fixture",
        }))
        .unwrap(),
        recorded_at: timestamp().into(),
    }
}

fn timestamp() -> &'static str {
    "2026-08-31T00:00:00Z"
}
