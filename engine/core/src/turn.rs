use std::{error::Error, fmt, num::NonZeroU32};

/// Runtime-supplied identity retained across suspension and resume.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TurnId(Box<str>);

impl TurnId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for TurnId {
    type Error = InvalidTurnId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(InvalidTurnId)
        } else {
            Ok(Self(value.into()))
        }
    }
}

impl TryFrom<String> for TurnId {
    type Error = InvalidTurnId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(InvalidTurnId)
        } else {
            Ok(Self(value.into_boxed_str()))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTurnId;

impl fmt::Display for InvalidTurnId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("turn identity cannot be empty")
    }
}

impl Error for InvalidTurnId {}

/// Immutable execution limits selected by Runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnLimits {
    max_iterations: NonZeroU32,
}

impl TurnLimits {
    pub const fn new(max_iterations: NonZeroU32) -> Self {
        Self { max_iterations }
    }

    pub const fn max_iterations(self) -> NonZeroU32 {
        self.max_iterations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuspensionReason {
    ApprovalRequired,
    PartialModelOutput,
    RateBudgetExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalReason {
    Answered,
    NoMoreToolCalls,
    BudgetExhausted,
    Cancelled,
    ProviderUnavailable,
    Failed,
    OperatorRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnStatus {
    Running,
    Suspended(SuspensionReason),
    Terminal(TerminalReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IterationDecision {
    Started { iteration: NonZeroU32 },
    Terminated(TerminalReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    NotRunning,
    NotSuspended,
    AlreadyTerminal,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotRunning => "turn is not running",
            Self::NotSuspended => "turn is not suspended",
            Self::AlreadyTerminal => "turn is already terminal",
        })
    }
}

impl Error for TransitionError {}

/// In-memory control projection for one bounded Agent execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnState {
    turn_id: TurnId,
    limits: TurnLimits,
    completed_iterations: u32,
    status: TurnStatus,
}

impl TurnState {
    pub fn new(turn_id: TurnId, limits: TurnLimits) -> Self {
        Self {
            turn_id,
            limits,
            completed_iterations: 0,
            status: TurnStatus::Running,
        }
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub const fn limits(&self) -> TurnLimits {
        self.limits
    }

    pub const fn completed_iterations(&self) -> u32 {
        self.completed_iterations
    }

    pub const fn status(&self) -> TurnStatus {
        self.status
    }

    pub fn begin_iteration(&mut self) -> Result<IterationDecision, TransitionError> {
        self.require_running()?;

        if self.completed_iterations == self.limits.max_iterations.get() {
            let reason = TerminalReason::BudgetExhausted;
            self.status = TurnStatus::Terminal(reason);
            return Ok(IterationDecision::Terminated(reason));
        }

        self.completed_iterations += 1;
        let iteration = NonZeroU32::new(self.completed_iterations)
            .expect("the counter was incremented from a value below a non-zero limit");
        Ok(IterationDecision::Started { iteration })
    }

    pub fn suspend(&mut self, reason: SuspensionReason) -> Result<(), TransitionError> {
        self.require_running()?;
        self.status = TurnStatus::Suspended(reason);
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), TransitionError> {
        match self.status {
            TurnStatus::Suspended(_) => {
                self.status = TurnStatus::Running;
                Ok(())
            }
            TurnStatus::Terminal(_) => Err(TransitionError::AlreadyTerminal),
            TurnStatus::Running => Err(TransitionError::NotSuspended),
        }
    }

    pub fn terminate(&mut self, reason: TerminalReason) -> Result<(), TransitionError> {
        if matches!(self.status, TurnStatus::Terminal(_)) {
            return Err(TransitionError::AlreadyTerminal);
        }

        self.status = TurnStatus::Terminal(reason);
        Ok(())
    }

    fn require_running(&self) -> Result<(), TransitionError> {
        match self.status {
            TurnStatus::Running => Ok(()),
            TurnStatus::Suspended(_) => Err(TransitionError::NotRunning),
            TurnStatus::Terminal(_) => Err(TransitionError::AlreadyTerminal),
        }
    }
}
