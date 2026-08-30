use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalEvidenceId, GoalEvidenceKind, GoalEvidenceV1, GoalId, GoalScopeV1, GoalSnapshot, GoalState,
    GoalTransition,
};
use serde_json::Value;

#[test]
fn shared_fixture_matches_canonical_definition_and_lifecycle() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/goal-lifecycle-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture["schema_version"], 1);
    let definition = definition(&fixture["definition"]);
    assert_eq!(
        definition.digest().unwrap(),
        fixture["definition"]["definition_digest"].as_str().unwrap()
    );
    let evidence = evidence(&fixture["evidence"]);
    let mut snapshot = GoalSnapshot::new(definition);
    for step in fixture["valid_sequence"].as_array().unwrap() {
        let transition = match step["operation"].as_str().unwrap() {
            "activate" => GoalTransition::Activate,
            "suspend" => GoalTransition::Suspend(step["reason"].as_str().unwrap().into()),
            "succeed" => GoalTransition::Succeed(vec![evidence.clone()]),
            _ => panic!("unknown fixture transition"),
        };
        snapshot = snapshot
            .apply(step["expected_revision"].as_u64().unwrap(), transition)
            .unwrap();
        assert_eq!(snapshot.revision(), step["revision"].as_u64().unwrap());
        assert_eq!(state_name(snapshot.state()), step["state"]);
    }
}

fn definition(value: &Value) -> GoalDefinitionV1 {
    let bounds = &value["bounds"];
    let scope = &value["scope"];
    let criterion = &value["criteria"][0];
    GoalDefinitionV1::new(
        GoalId::new(value["goal_id"].as_str().unwrap()).unwrap(),
        value["objective"].as_str().unwrap(),
        vec![GoalCriterion::UserAcceptance {
            criterion_id: GoalCriterionId::new(criterion["criterion_id"].as_str().unwrap())
                .unwrap(),
            response_schema_digest: criterion["response_schema_digest"].as_str().unwrap().into(),
        }],
        GoalScopeV1::new(
            scope["session_id"].as_str().map(str::to_owned),
            scope["workspace_capability_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item.as_str().unwrap().to_owned()),
        )
        .unwrap(),
        GoalBoundsV1::new(
            bounds["max_attempts"].as_u64().unwrap() as u32,
            bounds["max_plan_revisions"].as_u64().unwrap() as u32,
            bounds["max_child_goals"].as_u64().unwrap() as u32,
            bounds["token_budget"].as_u64(),
            bounds["duration_budget_ms"].as_u64(),
        )
        .unwrap(),
        None,
        value["capability_references"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reference| {
                GoalCapabilityReference::new(
                    reference["name"].as_str().unwrap(),
                    reference["exact_revision"].as_str().unwrap(),
                )
                .unwrap()
            }),
    )
    .unwrap()
}

fn evidence(value: &Value) -> GoalEvidenceV1 {
    GoalEvidenceV1::new(
        GoalEvidenceId::new(value["evidence_id"].as_str().unwrap()).unwrap(),
        GoalCriterionId::new(value["criterion_id"].as_str().unwrap()).unwrap(),
        GoalEvidenceKind::UserAcceptance,
        value["durable_reference"].as_str().unwrap(),
        value["evidence_digest"].as_str().unwrap(),
        value["observed_at_commit_version"].as_u64().unwrap(),
    )
    .unwrap()
}

const fn state_name(state: GoalState) -> &'static str {
    match state {
        GoalState::Draft => "draft",
        GoalState::Active => "active",
        GoalState::Suspended => "suspended",
        GoalState::Succeeded => "succeeded",
        GoalState::Failed => "failed",
        GoalState::Cancelled => "cancelled",
    }
}
