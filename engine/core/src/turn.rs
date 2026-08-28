use std::{error::Error, fmt, num::NonZeroU32};

macro_rules! identity_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = InvalidIdentity;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                if value.is_empty() {
                    Err(InvalidIdentity { kind: $label })
                } else {
                    Ok(Self(value.into()))
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidIdentity;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                if value.is_empty() {
                    Err(InvalidIdentity { kind: $label })
                } else {
                    Ok(Self(value.into_boxed_str()))
                }
            }
        }
    };
}

identity_type!(TurnId, "turn");
identity_type!(ExecutionId, "execution");
identity_type!(SessionId, "session");
identity_type!(AgentInstanceId, "agent instance");
identity_type!(AgentDefinitionId, "agent definition");
identity_type!(AgentDefinitionRevision, "agent definition revision");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidIdentity {
    kind: &'static str,
}

impl InvalidIdentity {
    pub const fn kind(self) -> &'static str {
        self.kind
    }
}

impl fmt::Display for InvalidIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} identity cannot be empty", self.kind)
    }
}

impl Error for InvalidIdentity {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionLimits {
    max_iterations: NonZeroU32,
}

impl ExecutionLimits {
    pub const fn new(max_iterations: NonZeroU32) -> Self {
        Self { max_iterations }
    }

    pub const fn max_iterations(self) -> NonZeroU32 {
        self.max_iterations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionOutcomeKind {
    Completed,
    Suspended,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionStatus {
    Active,
    Closed(ExecutionOutcomeKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeginIteration {
    Started { iteration: NonZeroU32 },
    IterationLimitReached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlError {
    CursorBeyondLimit { completed: u32, maximum: u32 },
    AlreadyClosed,
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CursorBeyondLimit { completed, maximum } => write!(
                formatter,
                "completed iteration cursor {completed} exceeds limit {maximum}"
            ),
            Self::AlreadyClosed => formatter.write_str("execution is already closed"),
        }
    }
}

impl Error for ControlError {}

/// Disposable control projection for one Kernel Execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionControl {
    turn_id: TurnId,
    execution_id: ExecutionId,
    limits: ExecutionLimits,
    completed_iterations: u32,
    status: ExecutionStatus,
}

impl ExecutionControl {
    pub fn new(
        turn_id: TurnId,
        execution_id: ExecutionId,
        completed_iterations: u32,
        limits: ExecutionLimits,
    ) -> Result<Self, ControlError> {
        let maximum = limits.max_iterations.get();
        if completed_iterations > maximum {
            return Err(ControlError::CursorBeyondLimit {
                completed: completed_iterations,
                maximum,
            });
        }

        Ok(Self {
            turn_id,
            execution_id,
            limits,
            completed_iterations,
            status: ExecutionStatus::Active,
        })
    }

    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    pub fn execution_id(&self) -> &ExecutionId {
        &self.execution_id
    }

    pub const fn limits(&self) -> ExecutionLimits {
        self.limits
    }

    pub const fn completed_iterations(&self) -> u32 {
        self.completed_iterations
    }

    pub const fn status(&self) -> ExecutionStatus {
        self.status
    }

    pub fn begin_iteration(&mut self) -> Result<BeginIteration, ControlError> {
        self.require_active()?;
        if self.completed_iterations == self.limits.max_iterations.get() {
            self.status = ExecutionStatus::Closed(ExecutionOutcomeKind::Stopped);
            return Ok(BeginIteration::IterationLimitReached);
        }

        self.completed_iterations += 1;
        let iteration = NonZeroU32::new(self.completed_iterations)
            .expect("a successfully incremented iteration is non-zero");
        Ok(BeginIteration::Started { iteration })
    }

    pub fn close(&mut self, kind: ExecutionOutcomeKind) -> Result<(), ControlError> {
        self.require_active()?;
        self.status = ExecutionStatus::Closed(kind);
        Ok(())
    }

    fn require_active(&self) -> Result<(), ControlError> {
        match self.status {
            ExecutionStatus::Active => Ok(()),
            ExecutionStatus::Closed(_) => Err(ControlError::AlreadyClosed),
        }
    }
}
