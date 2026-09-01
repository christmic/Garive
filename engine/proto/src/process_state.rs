use crate::com::garive::process::v1::{
    ProcessIdentityV1, ProcessProtocolFailureV1, ProcessServiceStateV1, ProcessStatusV1,
    ProcessTerminalReceiptV1,
};
use crate::process_receipt_digest;

/// Closed state-machine failures for one exact process dispatch attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStateError {
    /// The command does not carry the reducer's complete bound identity.
    IdentityMismatch,
    /// The transition is illegal or would replay a consumed start.
    StateConflict,
    /// Terminal evidence is incomplete, unbounded, or digest-mismatched.
    InvalidTerminal,
}

impl ProcessStateError {
    /// Maps reducer failures to the closed wire failure vocabulary.
    pub const fn protocol_failure(self) -> ProcessProtocolFailureV1 {
        match self {
            Self::IdentityMismatch => {
                ProcessProtocolFailureV1::ProcessProtocolFailureIdentityMismatch
            }
            Self::StateConflict => ProcessProtocolFailureV1::ProcessProtocolFailureStateConflict,
            Self::InvalidTerminal => ProcessProtocolFailureV1::ProcessProtocolFailureMalformed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnedState {
    Absent,
    Starting,
    Running,
    TerminalRetained(Box<ProcessTerminalReceiptV1>),
}

/// Pure fail-closed state reducer bound to one never-replayed dispatch identity.
#[derive(Clone, Debug)]
pub struct ProcessStateReducer {
    identity: ProcessIdentityV1,
    state: OwnedState,
    start_consumed: bool,
}

impl ProcessStateReducer {
    /// Creates an absent reducer for a fully validated identity.
    pub fn new(identity: ProcessIdentityV1) -> Result<Self, ProcessStateError> {
        if identity.workload_digest.len() != 32
            || identity.prepared_digest.len() != 32
            || identity.vm_configuration_digest.len() != 32
        {
            return Err(ProcessStateError::IdentityMismatch);
        }
        Ok(Self {
            identity,
            state: OwnedState::Absent,
            start_consumed: false,
        })
    }

    /// Consumes the sole start authority and enters starting.
    pub fn start(&mut self, identity: &ProcessIdentityV1) -> Result<(), ProcessStateError> {
        self.require_identity(identity)?;
        if self.start_consumed || self.state != OwnedState::Absent {
            return Err(ProcessStateError::StateConflict);
        }
        self.start_consumed = true;
        self.state = OwnedState::Starting;
        Ok(())
    }

    /// Marks that the exact guest workload may now be running.
    pub fn mark_running(&mut self, identity: &ProcessIdentityV1) -> Result<(), ProcessStateError> {
        self.require_identity(identity)?;
        if self.state != OwnedState::Starting {
            return Err(ProcessStateError::StateConflict);
        }
        self.state = OwnedState::Running;
        Ok(())
    }

    /// Validates and retains terminal evidence only after running.
    pub fn retain_terminal(
        &mut self,
        mut receipt: ProcessTerminalReceiptV1,
    ) -> Result<(), ProcessStateError> {
        let receipt_identity = receipt
            .identity
            .as_ref()
            .ok_or(ProcessStateError::InvalidTerminal)?;
        self.require_identity(receipt_identity)?;
        if self.state != OwnedState::Running {
            return Err(ProcessStateError::StateConflict);
        }
        let digest =
            process_receipt_digest(&receipt).map_err(|_| ProcessStateError::InvalidTerminal)?;
        receipt.receipt_digest = digest.to_vec();
        self.state = OwnedState::TerminalRetained(Box::new(receipt));
        Ok(())
    }

    /// Returns the exact externally visible status without changing state.
    pub fn query(
        &self,
        identity: &ProcessIdentityV1,
    ) -> Result<ProcessStatusV1, ProcessStateError> {
        self.require_identity(identity)?;
        let (state, terminal) = match &self.state {
            OwnedState::Absent => (ProcessServiceStateV1::ProcessServiceStateAbsent, None),
            OwnedState::Starting => (ProcessServiceStateV1::ProcessServiceStateStarting, None),
            OwnedState::Running => (ProcessServiceStateV1::ProcessServiceStateRunning, None),
            OwnedState::TerminalRetained(receipt) => (
                ProcessServiceStateV1::ProcessServiceStateTerminalRetained,
                Some((**receipt).clone()),
            ),
        };
        Ok(ProcessStatusV1 {
            identity: Some(self.identity.clone()),
            state: state.into(),
            terminal,
        })
    }

    /// Terminates starting/running ownership or proves exact idempotent absence.
    pub fn terminate(&mut self, identity: &ProcessIdentityV1) -> Result<(), ProcessStateError> {
        self.require_identity(identity)?;
        match self.state {
            OwnedState::Absent | OwnedState::Starting | OwnedState::Running => {
                self.state = OwnedState::Absent;
                Ok(())
            }
            OwnedState::TerminalRetained(_) => Err(ProcessStateError::StateConflict),
        }
    }

    /// Erases a retained receipt only for its exact digest and identity.
    pub fn acknowledge(
        &mut self,
        identity: &ProcessIdentityV1,
        receipt_digest: &[u8],
    ) -> Result<(), ProcessStateError> {
        self.require_identity(identity)?;
        match &self.state {
            OwnedState::TerminalRetained(receipt)
                if receipt_digest.len() == 32 && receipt.receipt_digest == receipt_digest =>
            {
                self.state = OwnedState::Absent;
                Ok(())
            }
            OwnedState::TerminalRetained(_) => Err(ProcessStateError::IdentityMismatch),
            _ => Err(ProcessStateError::StateConflict),
        }
    }

    fn require_identity(&self, identity: &ProcessIdentityV1) -> Result<(), ProcessStateError> {
        (identity == &self.identity)
            .then_some(())
            .ok_or(ProcessStateError::IdentityMismatch)
    }
}
