use garive_eval::{
    summarize_creativity, CreativityArm, CreativityArmEvidence, CreativityTaskClass,
    EvaluationCaseId, EvaluationErrorCode,
};

fn evidence(
    id: &str,
    class: CreativityTaskClass,
    arm: CreativityArm,
    candidates: u64,
    correct: u64,
    clusters: u64,
    selected: bool,
) -> CreativityArmEvidence {
    CreativityArmEvidence::new(
        EvaluationCaseId::new(id).unwrap(),
        class,
        arm,
        candidates,
        correct,
        clusters,
        selected,
    )
    .unwrap()
}

fn complete() -> Vec<CreativityArmEvidence> {
    CreativityTaskClass::ALL
        .into_iter()
        .enumerate()
        .flat_map(|(index, class)| {
            let id = format!("task-{index}");
            [
                evidence(&id, class, CreativityArm::Control, 1, 1, 1, true),
                evidence(
                    &id,
                    class,
                    CreativityArm::BoundedAlternatives,
                    3,
                    2,
                    2,
                    true,
                ),
            ]
        })
        .collect()
}

#[test]
fn complete_pairs_preserve_correctness_and_correct_only_diversity() {
    let summary = summarize_creativity(&complete(), 4).unwrap();
    assert_eq!(summary.ordered_pairs.len(), 4);
    assert_eq!(summary.classes.len(), 4);
    assert_eq!(summary.control.selected_correct_numerator, 4);
    assert_eq!(summary.control.selected_correct_denominator, 4);
    assert_eq!(summary.bounded_alternatives.candidate_count, 12);
    assert_eq!(summary.bounded_alternatives.correct_candidate_count, 8);
    assert_eq!(
        summary.bounded_alternatives.correct_cluster_mean_numerator,
        8
    );
    assert_eq!(
        summary
            .bounded_alternatives
            .correct_cluster_mean_denominator,
        4
    );
}

#[test]
fn invalid_arm_relations_coverage_order_and_overflow_fail_closed() {
    for result in [
        CreativityArmEvidence::new(
            EvaluationCaseId::new("bad").unwrap(),
            CreativityTaskClass::DesignAlternatives,
            CreativityArm::Control,
            2,
            1,
            1,
            true,
        ),
        CreativityArmEvidence::new(
            EvaluationCaseId::new("bad").unwrap(),
            CreativityTaskClass::DesignAlternatives,
            CreativityArm::BoundedAlternatives,
            2,
            1,
            2,
            false,
        ),
    ] {
        assert_eq!(
            result.unwrap_err().code(),
            EvaluationErrorCode::InvalidCreativityEvidence
        );
    }
    let mut values = complete();
    values.swap(0, 1);
    assert_eq!(
        summarize_creativity(&values, 4).unwrap_err().code(),
        EvaluationErrorCode::InvalidCreativityEvidence
    );
    assert_eq!(
        summarize_creativity(&complete()[..7], 4)
            .unwrap_err()
            .code(),
        EvaluationErrorCode::MissingCreativityEvidence
    );

    let mut overflow = complete();
    for value in overflow
        .iter_mut()
        .filter(|value| value.arm == CreativityArm::BoundedAlternatives)
    {
        value.candidate_count = u64::MAX;
    }
    assert_eq!(
        summarize_creativity(&overflow, 4).unwrap_err().code(),
        EvaluationErrorCode::CreativityArithmeticOverflow
    );
}
