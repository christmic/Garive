use std::collections::BTreeSet;

use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanErrorCode, PlanId, PlanStepId,
    PlanStepV1,
};

#[test]
fn valid_definition_is_canonical_and_order_sensitive() {
    let first = definition(valid_steps()).unwrap();
    let mut reversed = valid_steps();
    reversed.reverse();
    let second = definition(reversed).unwrap();
    assert_eq!(first.digest().unwrap().len(), 64);
    assert_ne!(first.digest().unwrap(), second.digest().unwrap());
    assert_eq!(
        first
            .step_digest(&PlanStepId::new("prepare").unwrap())
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn cycles_and_unknown_dependencies_fail_with_distinct_boundaries() {
    let cycle = vec![
        step("a", ["b"], ["accepted"]),
        step("b", ["a"], ["artifact"]),
    ];
    assert_eq!(
        definition(cycle).unwrap_err().code(),
        PlanErrorCode::PlanCycle
    );
    assert_eq!(
        definition(vec![step("a", ["missing"], ["accepted"])])
            .unwrap_err()
            .code(),
        PlanErrorCode::PlanInvalid
    );
}

#[test]
fn criterion_coverage_and_capability_scope_fail_closed() {
    assert_eq!(
        definition(vec![step("a", [], ["accepted"])])
            .unwrap_err()
            .code(),
        PlanErrorCode::PlanInvalid
    );
    let unavailable = PlanStepV1::new(
        PlanStepId::new("a").unwrap(),
        "Use unavailable capability",
        [],
        ["accepted".into(), "artifact".into()],
        [PlanCapabilityReference::new("browser", "native-v1").unwrap()],
        [digest('d')],
        1,
    )
    .unwrap();
    assert_eq!(
        definition(vec![unavailable]).unwrap_err().code(),
        PlanErrorCode::PlanInvalid
    );
}

fn definition(steps: Vec<PlanStepV1>) -> Result<PlanDefinitionV1, garive_plan::PlanError> {
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
        &BTreeSet::from([PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()]),
    )
}

fn valid_steps() -> Vec<PlanStepV1> {
    vec![
        step("prepare", [], ["accepted"]),
        step("deliver", ["prepare"], ["artifact"]),
    ]
}

fn step<const D: usize, const C: usize>(
    id: &str,
    dependencies: [&str; D],
    criteria: [&str; C],
) -> PlanStepV1 {
    PlanStepV1::new(
        PlanStepId::new(id).unwrap(),
        format!("Complete {id}"),
        dependencies
            .into_iter()
            .map(|value| PlanStepId::new(value).unwrap()),
        criteria.into_iter().map(str::to_owned),
        [PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()],
        [digest('d')],
        2,
    )
    .unwrap()
}

fn set<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
