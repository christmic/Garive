use std::collections::BTreeSet;

use garive_plan::{
    PlanBoundsV1, PlanCapabilityReference, PlanDefinitionV1, PlanErrorCode, PlanId, PlanSnapshot,
    PlanState, PlanStepId, PlanStepV1, PlanTransition, StepState,
};

#[test]
fn adopted_dag_unlocks_in_declaration_order_and_completes_explicitly() {
    let prepare = id("prepare");
    let deliver = id("deliver");
    let adopted = PlanSnapshot::new(definition(2, 4, 2))
        .apply(PlanTransition::Adopt)
        .unwrap();
    assert_eq!(adopted.ready_steps(), vec![&prepare]);

    let running = adopted
        .apply(PlanTransition::Claim(prepare.clone()))
        .unwrap()
        .apply(PlanTransition::Start(prepare.clone()))
        .unwrap();
    assert_eq!(running.state(), PlanState::Running);
    assert_eq!(running.step(&prepare).unwrap().attempts(), 1);
    let progressed = running
        .apply(PlanTransition::CompleteStep(prepare))
        .unwrap();
    assert_eq!(progressed.ready_steps(), vec![&deliver]);
    let finished_steps = progressed
        .apply(PlanTransition::Claim(deliver.clone()))
        .unwrap()
        .apply(PlanTransition::Start(deliver.clone()))
        .unwrap()
        .apply(PlanTransition::CompleteStep(deliver))
        .unwrap();
    assert_eq!(
        finished_steps
            .apply(PlanTransition::Complete {
                criteria_complete: false,
            })
            .unwrap_err()
            .code(),
        PlanErrorCode::PlanTransitionInvalid
    );
    let completed = finished_steps
        .apply(PlanTransition::Complete {
            criteria_complete: true,
        })
        .unwrap();
    assert_eq!(completed.state(), PlanState::Completed);
}

#[test]
fn claim_capacity_and_attempt_limits_fail_closed() {
    let first = id("first");
    let second = id("second");
    let adopted = PlanSnapshot::new(independent_definition(1, 1))
        .apply(PlanTransition::Adopt)
        .unwrap();
    assert_eq!(adopted.ready_steps(), vec![&first, &second]);
    let claimed = adopted.apply(PlanTransition::Claim(first.clone())).unwrap();
    assert_eq!(
        claimed
            .apply(PlanTransition::Claim(second))
            .unwrap_err()
            .code(),
        PlanErrorCode::StepNotReady
    );
    let failed = claimed
        .apply(PlanTransition::Start(first.clone()))
        .unwrap()
        .apply(PlanTransition::FailStep(first.clone()))
        .unwrap();
    assert_eq!(failed.step(&first).unwrap().state(), StepState::Failed);
    assert_eq!(
        failed
            .apply(PlanTransition::RetryStep(first))
            .unwrap_err()
            .code(),
        PlanErrorCode::PlanBoundExceeded
    );
}

#[test]
fn never_started_claim_can_expire_but_started_attempt_cannot_replay() {
    let prepare = id("prepare");
    let claimed = PlanSnapshot::new(definition(2, 4, 2))
        .apply(PlanTransition::Adopt)
        .unwrap()
        .apply(PlanTransition::Claim(prepare.clone()))
        .unwrap();
    let ready = claimed
        .apply(PlanTransition::ExpireClaim(prepare.clone()))
        .unwrap();
    assert_eq!(ready.step(&prepare).unwrap().state(), StepState::Ready);
    let running = ready
        .apply(PlanTransition::Claim(prepare.clone()))
        .unwrap()
        .apply(PlanTransition::Start(prepare.clone()))
        .unwrap();
    assert_eq!(
        running
            .apply(PlanTransition::ExpireClaim(prepare))
            .unwrap_err()
            .code(),
        PlanErrorCode::PlanTransitionInvalid
    );
}

#[test]
fn carry_forward_adopts_only_dependency_closed_completed_steps() {
    let prepare = id("prepare");
    let deliver = id("deliver");
    let carried = PlanSnapshot::new(definition(2, 4, 2))
        .apply(PlanTransition::AdoptWithCarryForward(BTreeSet::from([
            prepare.clone(),
        ])))
        .unwrap();
    assert_eq!(carried.state(), PlanState::Running);
    assert_eq!(
        carried.step(&prepare).unwrap().state(),
        StepState::Completed
    );
    assert_eq!(carried.step(&prepare).unwrap().attempts(), 0);
    assert_eq!(carried.ready_steps(), vec![&deliver]);
    assert_eq!(
        PlanSnapshot::new(definition(2, 4, 2))
            .apply(PlanTransition::AdoptWithCarryForward(BTreeSet::from([
                deliver,
            ])))
            .unwrap_err()
            .code(),
        PlanErrorCode::PlanTransitionInvalid
    );
}

fn definition(parallel: u32, total_attempts: u32, step_attempts: u32) -> PlanDefinitionV1 {
    build(
        vec![
            step("prepare", [], ["accepted"], step_attempts),
            step("deliver", ["prepare"], ["artifact"], step_attempts),
        ],
        parallel,
        total_attempts,
    )
}

fn independent_definition(parallel: u32, step_attempts: u32) -> PlanDefinitionV1 {
    build(
        vec![
            step("first", [], ["accepted"], step_attempts),
            step("second", [], ["artifact"], step_attempts),
        ],
        parallel,
        4,
    )
}

fn build(steps: Vec<PlanStepV1>, parallel: u32, total_attempts: u32) -> PlanDefinitionV1 {
    let capabilities = BTreeSet::from([capability()]);
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
        PlanBoundsV1::new(4, parallel, total_attempts, None, None).unwrap(),
        &set(["accepted", "artifact"]),
        &BTreeSet::new(),
        &capabilities,
    )
    .unwrap()
}

fn step<const D: usize, const C: usize>(
    value: &str,
    dependencies: [&str; D],
    criteria: [&str; C],
    max_attempts: u32,
) -> PlanStepV1 {
    PlanStepV1::new(
        id(value),
        format!("Complete {value}"),
        dependencies.into_iter().map(id),
        criteria.into_iter().map(str::to_owned),
        [capability()],
        [digest('d')],
        max_attempts,
    )
    .unwrap()
}

fn id(value: &str) -> PlanStepId {
    PlanStepId::new(value).unwrap()
}

fn capability() -> PlanCapabilityReference {
    PlanCapabilityReference::new("tools", "catalogue-v1").unwrap()
}

fn set<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
