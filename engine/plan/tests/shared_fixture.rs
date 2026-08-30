use std::{collections::BTreeSet, fs, path::PathBuf};

use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanId, PlanSnapshot, PlanState,
    PlanStepId, PlanStepV1, PlanTransition,
};
use serde_json::Value;

#[test]
fn shared_fixture_matches_canonical_definition_and_lifecycle() {
    let fixture: Value = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/agent/plan-lifecycle-v1.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let definition = definition();
    assert_eq!(
        definition.canonical_json().unwrap(),
        fixture["definition"]["canonical_json"].as_str().unwrap()
    );
    assert_eq!(
        definition.digest().unwrap(),
        fixture["definition"]["digest"].as_str().unwrap()
    );
    let capabilities =
        BTreeSet::from([PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()]);
    let decoded = PlanDefinitionV1::from_canonical_json(
        fixture["definition"]["canonical_json"].as_str().unwrap(),
        &set(["accepted", "artifact"]),
        &BTreeSet::new(),
        &capabilities,
    )
    .unwrap();
    assert_eq!(decoded, definition);

    let mut snapshot = PlanSnapshot::new(definition);
    let steps = fixture["valid_lifecycle"].as_array().unwrap();
    assert_eq!(steps.len(), 8);
    for step in steps {
        let transition = match step["transition"].as_str().unwrap() {
            "adopt" => PlanTransition::Adopt,
            "claim" => PlanTransition::Claim(step_id(step)),
            "start" => PlanTransition::Start(step_id(step)),
            "complete_step" => PlanTransition::CompleteStep(step_id(step)),
            "complete" => PlanTransition::Complete {
                criteria_complete: step["criteria_complete"].as_bool().unwrap(),
            },
            _ => panic!("unknown fixture transition"),
        };
        snapshot = snapshot.apply(transition).unwrap();
        assert_eq!(
            snapshot.state(),
            plan_state(step["plan_state"].as_str().unwrap())
        );
        assert_eq!(
            snapshot
                .ready_steps()
                .into_iter()
                .map(PlanStepId::as_str)
                .collect::<Vec<_>>(),
            step["ready"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            snapshot.total_attempts(),
            step["total_attempts"].as_u64().unwrap() as u32
        );
    }
}

fn definition() -> PlanDefinitionV1 {
    let capability = PlanCapabilityReference::new("tools", "catalogue-v1").unwrap();
    let steps = vec![
        PlanStepV1::new(
            PlanStepId::new("prepare").unwrap(),
            "Complete prepare",
            [],
            ["accepted".into()],
            [capability.clone()],
            [digest('d')],
            2,
        )
        .unwrap(),
        PlanStepV1::new(
            PlanStepId::new("deliver").unwrap(),
            "Complete deliver",
            [PlanStepId::new("prepare").unwrap()],
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
        PlanBoundsV1::new(4, 2, 6, Some(10_000), Some(60_000)).unwrap(),
        &set(["accepted", "artifact"]),
        &BTreeSet::new(),
        &BTreeSet::from([capability]),
    )
    .unwrap()
}

fn step_id(value: &Value) -> PlanStepId {
    PlanStepId::new(value["step_id"].as_str().unwrap()).unwrap()
}

fn plan_state(value: &str) -> PlanState {
    match value {
        "adopted" => PlanState::Adopted,
        "running" => PlanState::Running,
        "completed" => PlanState::Completed,
        _ => panic!("unknown fixture state"),
    }
}

fn set<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
