use std::num::NonZeroU32;

use garive_core::{
    BeginIteration, ControlError, ExecutionControl, ExecutionId, ExecutionLimits,
    ExecutionOutcomeKind, ExecutionStatus, TurnId,
};

fn control(completed: u32, maximum: u32) -> ExecutionControl {
    ExecutionControl::new(
        TurnId::try_from("turn-1").unwrap(),
        ExecutionId::try_from("execution-1").unwrap(),
        completed,
        ExecutionLimits::new(NonZeroU32::new(maximum).unwrap()),
    )
    .unwrap()
}

#[test]
fn identities_are_distinct_and_non_empty() {
    assert_eq!(TurnId::try_from("").unwrap_err().kind(), "turn");
    assert_eq!(ExecutionId::try_from("").unwrap_err().kind(), "execution");
    assert_eq!(TurnId::try_from("same").unwrap().as_str(), "same");
    assert_eq!(ExecutionId::try_from("same").unwrap().as_str(), "same");
}

#[test]
fn reconstructed_cursor_cannot_exceed_limit() {
    let result = ExecutionControl::new(
        TurnId::try_from("turn-1").unwrap(),
        ExecutionId::try_from("execution-2").unwrap(),
        3,
        ExecutionLimits::new(NonZeroU32::new(2).unwrap()),
    );
    assert_eq!(
        result,
        Err(ControlError::CursorBeyondLimit {
            completed: 3,
            maximum: 2
        })
    );
}

#[test]
fn limit_closes_without_overcounting() {
    let mut execution = control(1, 2);
    assert_eq!(
        execution.begin_iteration(),
        Ok(BeginIteration::Started {
            iteration: NonZeroU32::new(2).unwrap()
        })
    );
    assert_eq!(
        execution.begin_iteration(),
        Ok(BeginIteration::IterationLimitReached)
    );
    assert_eq!(execution.completed_iterations(), 2);
    assert_eq!(
        execution.status(),
        ExecutionStatus::Closed(ExecutionOutcomeKind::Stopped)
    );
}

#[test]
fn all_outcome_kinds_close_exactly_once() {
    for kind in [
        ExecutionOutcomeKind::Completed,
        ExecutionOutcomeKind::Suspended,
        ExecutionOutcomeKind::Stopped,
        ExecutionOutcomeKind::Failed,
    ] {
        let mut execution = control(0, 1);
        execution.close(kind).unwrap();
        let closed = execution.clone();
        assert_eq!(execution.close(kind), Err(ControlError::AlreadyClosed));
        assert_eq!(
            execution.begin_iteration(),
            Err(ControlError::AlreadyClosed)
        );
        assert_eq!(execution, closed);
    }
}

#[test]
fn continuation_is_a_new_execution_with_a_durable_cursor() {
    let mut first = control(0, 3);
    first.begin_iteration().unwrap();
    first.close(ExecutionOutcomeKind::Suspended).unwrap();

    let continued = ExecutionControl::new(
        first.turn_id().clone(),
        ExecutionId::try_from("execution-2").unwrap(),
        first.completed_iterations(),
        first.limits(),
    )
    .unwrap();

    assert_eq!(continued.turn_id(), first.turn_id());
    assert_ne!(continued.execution_id(), first.execution_id());
    assert_eq!(continued.completed_iterations(), 1);
    assert_eq!(continued.status(), ExecutionStatus::Active);
}
