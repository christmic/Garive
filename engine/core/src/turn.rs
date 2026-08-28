use std::{error::Error, fmt, num::NonZeroU32};

macro_rules! identity_type {
    ($name:ident, $label:literal) => {
        #[doc = concat!("Validated, non-empty ", $label, " identity.")]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            /// Returns the identity exactly as supplied by its authority.
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
/// Error returned when an identity is constructed from an empty value.
pub struct InvalidIdentity {
    kind: &'static str,
}

impl InvalidIdentity {
    /// Returns the human-readable identity kind rejected by validation.
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
/// Hard limits applied to one kernel execution.
pub struct ExecutionLimits {
    max_iterations: NonZeroU32,
}

impl ExecutionLimits {
    /// Creates limits with a non-zero maximum iteration count.
    pub const fn new(max_iterations: NonZeroU32) -> Self {
        Self { max_iterations }
    }

    /// Returns the maximum number of iterations the execution may start.
    pub const fn max_iterations(self) -> NonZeroU32 {
        self.max_iterations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Terminal class recorded when an execution closes.
pub enum ExecutionOutcomeKind {
    /// The agent produced its intended result.
    Completed,
    /// The agent preserved resumable work and awaits external progress.
    Suspended,
    /// A policy boundary stopped otherwise valid work.
    Stopped,
    /// Execution ended because an invariant or dependency failed.
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Lifecycle status of an execution control projection.
pub enum ExecutionStatus {
    /// The execution may start another iteration or be explicitly closed.
    Active,
    /// The execution has closed exactly once with the supplied outcome class.
    Closed(ExecutionOutcomeKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Result of attempting to enter the next bounded iteration.
pub enum BeginIteration {
    /// The iteration cursor advanced to this one-based value.
    Started {
        /// One-based iteration number entered by the controller.
        iteration: NonZeroU32,
    },
    /// No iteration started and the controller closed as stopped.
    IterationLimitReached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Rejected state transition in the execution controller.
pub enum ControlError {
    /// A resumed durable cursor is already beyond its declared limit.
    CursorBeyondLimit {
        /// Iterations recorded as completed by durable state.
        completed: u32,
        /// Maximum iterations allowed by the request.
        maximum: u32,
    },
    /// A transition was attempted after the execution had closed.
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
    /// Restores an active controller from a durable completed-iteration cursor.
    ///
    /// Returns [`ControlError::CursorBeyondLimit`] if the durable cursor cannot
    /// be reconciled with `limits`.
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

    /// Returns the owning turn identity.
    pub fn turn_id(&self) -> &TurnId {
        &self.turn_id
    }

    /// Returns this attempt's execution identity.
    pub fn execution_id(&self) -> &ExecutionId {
        &self.execution_id
    }

    /// Returns the immutable execution limits.
    pub const fn limits(&self) -> ExecutionLimits {
        self.limits
    }

    /// Returns the number of iterations entered so far.
    pub const fn completed_iterations(&self) -> u32 {
        self.completed_iterations
    }

    /// Returns whether execution is active or terminal.
    pub const fn status(&self) -> ExecutionStatus {
        self.status
    }

    /// Starts the next iteration or closes the execution at its iteration cap.
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

    /// Closes an active execution with one terminal outcome class.
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
