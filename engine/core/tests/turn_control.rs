use std::num::NonZeroU32;

use garive_core::{
    IterationDecision, SuspensionReason, TerminalReason, TransitionError, TurnId, TurnLimits,
    TurnState, TurnStatus,
};

fn state(max_iterations: u32) -> TurnState {
    TurnState::new(
        TurnId::try_from("turn-1").unwrap(),
        TurnLimits::new(NonZeroU32::new(max_iterations).unwrap()),
    )
}

#[test]
fn identity_rejects_empty_values() {
    assert!(TurnId::try_from("").is_err());
    assert_eq!(TurnId::try_from("turn-1").unwrap().as_str(), "turn-1");
}

#[test]
fn iteration_limit_terminates_without_overcounting() {
    let mut turn = state(2);

    assert_eq!(
        turn.begin_iteration(),
        Ok(IterationDecision::Started {
            iteration: NonZeroU32::new(1).unwrap()
        })
    );
    assert_eq!(
        turn.begin_iteration(),
        Ok(IterationDecision::Started {
            iteration: NonZeroU32::new(2).unwrap()
        })
    );
    assert_eq!(
        turn.begin_iteration(),
        Ok(IterationDecision::Terminated(
            TerminalReason::BudgetExhausted
        ))
    );
    assert_eq!(turn.completed_iterations(), 2);
    assert_eq!(
        turn.status(),
        TurnStatus::Terminal(TerminalReason::BudgetExhausted)
    );
}

#[test]
fn suspend_and_resume_preserve_control_identity() {
    let mut turn = state(3);
    turn.begin_iteration().unwrap();
    let identity = turn.turn_id().clone();
    let limits = turn.limits();

    turn.suspend(SuspensionReason::ApprovalRequired).unwrap();
    assert_eq!(
        turn.status(),
        TurnStatus::Suspended(SuspensionReason::ApprovalRequired)
    );
    turn.resume().unwrap();

    assert_eq!(turn.status(), TurnStatus::Running);
    assert_eq!(turn.turn_id(), &identity);
    assert_eq!(turn.limits(), limits);
    assert_eq!(turn.completed_iterations(), 1);
}

#[test]
fn invalid_transitions_do_not_mutate_state() {
    let mut running = state(1);
    let before = running.clone();
    assert_eq!(running.resume(), Err(TransitionError::NotSuspended));
    assert_eq!(running, before);

    running
        .suspend(SuspensionReason::PartialModelOutput)
        .unwrap();
    let before = running.clone();
    assert_eq!(running.begin_iteration(), Err(TransitionError::NotRunning));
    assert_eq!(running, before);
}

#[test]
fn every_terminal_reason_is_immutable() {
    let reasons = [
        TerminalReason::Answered,
        TerminalReason::NoMoreToolCalls,
        TerminalReason::BudgetExhausted,
        TerminalReason::Cancelled,
        TerminalReason::ProviderUnavailable,
        TerminalReason::Failed,
        TerminalReason::OperatorRequired,
    ];

    for reason in reasons {
        let mut turn = state(1);
        turn.terminate(reason).unwrap();
        let terminal = turn.clone();

        assert_eq!(
            turn.begin_iteration(),
            Err(TransitionError::AlreadyTerminal)
        );
        assert_eq!(
            turn.suspend(SuspensionReason::ApprovalRequired),
            Err(TransitionError::AlreadyTerminal)
        );
        assert_eq!(turn.resume(), Err(TransitionError::AlreadyTerminal));
        assert_eq!(
            turn.terminate(TerminalReason::Failed),
            Err(TransitionError::AlreadyTerminal)
        );
        assert_eq!(turn, terminal);
    }
}
