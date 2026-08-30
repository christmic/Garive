use garive_goal::{
    GoalBoundsV1, GoalCapabilityReference, GoalCriterion, GoalCriterionId, GoalDefinitionV1,
    GoalErrorCode, GoalEvidenceId, GoalEvidenceKind, GoalEvidenceV1, GoalId, GoalScopeV1,
    GoalSnapshot, GoalState, GoalTransition,
};

#[test]
fn activation_suspension_and_resume_advance_contiguous_revisions() {
    let draft = GoalSnapshot::new(definition("goal-1", "Ship the slice"));
    let active = draft.apply(1, GoalTransition::Activate).unwrap();
    let suspended = active
        .apply(2, GoalTransition::Suspend("approval_required".into()))
        .unwrap();
    let resumed = suspended.apply(3, GoalTransition::Activate).unwrap();
    assert_eq!(
        (resumed.revision(), resumed.state()),
        (4, GoalState::Active)
    );
}

#[test]
fn stale_revision_and_invalid_edges_fail_without_mutation() {
    let draft = GoalSnapshot::new(definition("goal-1", "Ship the slice"));
    assert_eq!(
        draft.apply(2, GoalTransition::Activate).unwrap_err().code(),
        GoalErrorCode::GoalRevisionConflict
    );
    assert_eq!(
        draft
            .apply(1, GoalTransition::Succeed(vec![evidence()]))
            .unwrap_err()
            .code(),
        GoalErrorCode::GoalTransitionInvalid
    );
}

#[test]
fn success_requires_exact_kind_and_complete_evidence() {
    let active = GoalSnapshot::new(definition("goal-1", "Ship the slice"))
        .apply(1, GoalTransition::Activate)
        .unwrap();
    assert_eq!(
        active
            .apply(2, GoalTransition::Succeed(Vec::new()))
            .unwrap_err()
            .code(),
        GoalErrorCode::GoalEvidenceInsufficient
    );
    let wrong = GoalEvidenceV1::new(
        GoalEvidenceId::new("evidence-1").unwrap(),
        GoalCriterionId::new("accepted").unwrap(),
        GoalEvidenceKind::Artifact,
        "artifact-1",
        digest('b'),
        5,
    )
    .unwrap();
    assert_eq!(
        active
            .apply(2, GoalTransition::Succeed(vec![wrong]))
            .unwrap_err()
            .code(),
        GoalErrorCode::GoalEvidenceInsufficient
    );
}

#[test]
fn verified_success_is_terminal() {
    let succeeded = GoalSnapshot::new(definition("goal-1", "Ship the slice"))
        .apply(1, GoalTransition::Activate)
        .unwrap()
        .apply(2, GoalTransition::Succeed(vec![evidence()]))
        .unwrap();
    assert_eq!(
        (succeeded.revision(), succeeded.state()),
        (3, GoalState::Succeeded)
    );
    assert_eq!(
        succeeded
            .apply(3, GoalTransition::Cancel("changed_mind".into()))
            .unwrap_err()
            .code(),
        GoalErrorCode::GoalTransitionInvalid
    );
}

#[test]
fn revision_replaces_definition_but_not_goal_identity() {
    let active = GoalSnapshot::new(definition("goal-1", "First objective"))
        .apply(1, GoalTransition::Activate)
        .unwrap();
    let revised = active
        .apply(
            2,
            GoalTransition::Revise(Box::new(definition("goal-1", "Revised objective"))),
        )
        .unwrap();
    assert_eq!((revised.revision(), revised.state()), (3, GoalState::Draft));
    assert_eq!(
        revised
            .apply(
                3,
                GoalTransition::Revise(Box::new(definition("goal-2", "Wrong identity"))),
            )
            .unwrap_err()
            .code(),
        GoalErrorCode::GoalTransitionInvalid
    );
}

#[test]
fn definition_digest_is_canonical_and_objective_sensitive() {
    assert_eq!(
        definition("goal-1", "Ship the slice").digest().unwrap(),
        "fa3b251bc520d2d66080f27ab1827b1c36e89c71a59f740e8570a77c8b42fe76"
    );
    assert_ne!(
        definition("goal-1", "Ship the slice").digest().unwrap(),
        definition("goal-1", "Ship another slice").digest().unwrap()
    );
}

#[test]
fn canonical_definition_round_trips_and_noncanonical_input_is_rejected() {
    let original = definition("goal-1", "Ship the slice");
    let json = original.canonical_json().unwrap();
    let decoded = GoalDefinitionV1::from_canonical_json(&json).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(
        GoalDefinitionV1::from_canonical_json(&format!(" {json}"))
            .unwrap_err()
            .code(),
        GoalErrorCode::GoalInvalid
    );
}

fn definition(goal_id: &str, objective: &str) -> GoalDefinitionV1 {
    GoalDefinitionV1::new(
        GoalId::new(goal_id).unwrap(),
        objective,
        vec![GoalCriterion::UserAcceptance {
            criterion_id: GoalCriterionId::new("accepted").unwrap(),
            response_schema_digest: digest('a'),
        }],
        GoalScopeV1::new(Some("session-1".into()), ["workspace-1".into()]).unwrap(),
        GoalBoundsV1::new(3, 4, 2, Some(10_000), Some(60_000)).unwrap(),
        None,
        [GoalCapabilityReference::new("tools", "catalogue-v1").unwrap()],
    )
    .unwrap()
}

fn evidence() -> GoalEvidenceV1 {
    GoalEvidenceV1::new(
        GoalEvidenceId::new("evidence-1").unwrap(),
        GoalCriterionId::new("accepted").unwrap(),
        GoalEvidenceKind::UserAcceptance,
        "interaction-1",
        digest('b'),
        5,
    )
    .unwrap()
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}
