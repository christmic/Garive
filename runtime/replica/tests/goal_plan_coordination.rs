use std::collections::BTreeSet;

use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalId, GoalScopeV1, GoalState,
};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanStepId, PlanStepV1,
};
use garive_runtime::{
    commit_goal_command, commit_plan_command, plan_activate_goal_from_authoritative_plan,
    plan_adopt_plan, plan_create_goal, plan_propose_plan, reconstruct_goal, reconstruct_plan_graph,
    GoalCommandContext, GoalPlanCoordinationError, PlanCommandContext, PlanRuntimeTransition,
    SqliteLedger,
};
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn activation_derives_the_unique_adopted_plan_reference_from_the_ledger() {
    let directory = tempdir().unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("coordination.sqlite3")).unwrap();
    let session = SessionId::try_from("session-1").unwrap();
    ledger
        .commit(
            session.clone(),
            0,
            vec![fact("session-open", "session.opened", json!({}))],
        )
        .unwrap();

    assert_eq!(
        plan_activate_goal_from_authoritative_plan(
            &ledger,
            &session,
            "goal-1",
            1,
            1,
            &goal_context("activate-missing"),
        ),
        Err(GoalPlanCoordinationError::Goal(
            garive_runtime::GoalRuntimeError::NotFound
        ))
    );

    let created =
        plan_create_goal(&ledger, &session, &goal_context("goal-create"), goal()).unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();
    assert_eq!(
        plan_activate_goal_from_authoritative_plan(
            &ledger,
            &session,
            "goal-1",
            2,
            1,
            &goal_context("activate-without-plan"),
        ),
        Err(GoalPlanCoordinationError::AuthoritativePlanUnavailable)
    );

    let proposed =
        plan_propose_plan(&ledger, &session, &plan_context("plan-propose"), plan()).unwrap();
    commit_plan_command(&mut ledger, session.clone(), 2, &proposed).unwrap();
    let proposed_state = reconstruct_plan_graph(&ledger, &session)
        .unwrap()
        .remove(&("plan-1".into(), 1))
        .unwrap();
    let adopted = plan_adopt_plan(
        &ledger,
        &session,
        &proposed_state,
        proposed_state.state_version,
        &plan_context("plan-adopt"),
        PlanRuntimeTransition::Adopt {
            expected_goal_revision: 1,
            expected_prior_plan_revision: None,
            policy_reference: "policy:default-planning-v1".into(),
            carry_forward_evidence: CanonicalPayload::from_value(&json!([])).unwrap(),
        },
    )
    .unwrap();
    commit_plan_command(&mut ledger, session.clone(), 3, &adopted).unwrap();

    let activation = plan_activate_goal_from_authoritative_plan(
        &ledger,
        &session,
        "goal-1",
        4,
        1,
        &goal_context("goal-activate"),
    )
    .unwrap();
    let payload: Value = serde_json::from_str(activation.facts[0].payload.as_json()).unwrap();
    assert_eq!(payload["actor_reference"], "runtime:goal-plan-coordinator");
    let reference: Value =
        serde_json::from_str(payload["plan_reference"].as_str().unwrap()).unwrap();
    assert_eq!(reference["plan_id"], "plan-1");
    assert_eq!(reference["revision"], 1);
    assert_eq!(reference["definition_digest"], plan().digest().unwrap());

    commit_goal_command(&mut ledger, session.clone(), 4, &activation).unwrap();
    let active = reconstruct_goal(&ledger, &session, "goal-1").unwrap();
    assert_eq!(active.snapshot.state(), GoalState::Active);
    assert_eq!(active.snapshot.revision(), 2);
}

fn goal() -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new("goal-1").unwrap(),
        "Deliver one verified artifact",
        vec![GoalCriterion::Artifact {
            criterion_id: GoalCriterionId::new("artifact").unwrap(),
            artifact_kind: "file".into(),
            required_digest: None,
        }],
        GoalScopeV1::new(Some("session-1".into()), []).unwrap(),
        GoalBoundsV1::new(1, 2, 1, None, None).unwrap(),
        None,
        [GoalCapabilityReference::new("tools", "catalogue-v1").unwrap()],
    )
    .unwrap()
}

fn plan() -> PlanDefinitionV1 {
    let capability = PlanCapabilityReference::new("tools", "catalogue-v1").unwrap();
    PlanDefinitionV1::new(
        PlanId::new("plan-1").unwrap(),
        1,
        "goal-1",
        1,
        goal().digest().unwrap(),
        digest('a'),
        digest('b'),
        "safety-v1",
        vec![PlanStepV1::new(
            PlanStepId::new("deliver").unwrap(),
            "Deliver",
            [],
            ["artifact".into()],
            [capability.clone()],
            [digest('c')],
            1,
        )
        .unwrap()],
        PlanBoundsV1::new(1, 1, 1, None, None).unwrap(),
        &BTreeSet::from(["artifact".into()]),
        &BTreeSet::new(),
        &BTreeSet::from([capability]),
    )
    .unwrap()
}

fn goal_context(command_id: &str) -> GoalCommandContext {
    GoalCommandContext {
        command_id: command_id.into(),
        actor_reference: "runtime:goal-plan-coordinator".into(),
        recorded_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn plan_context(command_id: &str) -> PlanCommandContext {
    PlanCommandContext {
        command_id: command_id.into(),
        actor_reference: "runtime:goal-plan-coordinator".into(),
        recorded_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn fact(id: &str, kind: &str, payload: Value) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
