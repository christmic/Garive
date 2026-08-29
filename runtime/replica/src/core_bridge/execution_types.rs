use std::{error::Error, fmt};

use garive_core::ExecutionReport;
use garive_ledger::{CommitResult, SessionId};

use crate::{ExecutionLeaseError, ExecutionLeaseRequest, RuntimeCommandError, SqliteLedgerError};

use super::ModelLifecycleContext;

/// Immutable persistence and model policy for one already-started Execution.
pub struct DurableExecutionConfig {
    /// Durable Session owning all appended facts.
    pub session_id: SessionId,
    /// Session version immediately after `execution.started` committed.
    pub expected_session_version: u64,
    /// Exact model lifecycle configuration frozen for this Execution.
    pub model: ModelLifecycleContext,
    /// Explicit operational lease acquired before Core may execute.
    pub lease: ExecutionLeaseRequest,
}

/// Failure before a durable terminal transaction can be committed.
#[derive(Debug)]
pub enum DurableExecutionError {
    /// Core/configuration identities or lifecycle values conflict.
    Command(RuntimeCommandError),
    /// SQLite or Ledger rejected a required durability boundary.
    Ledger(SqliteLedgerError),
    /// The Execution lease could not be acquired, retained, or released safely.
    Lease(ExecutionLeaseError),
    /// An internal synchronization boundary was poisoned.
    Coordination,
}

/// Redacted failure from terminal Host publication after commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalPublicationError;

/// Host boundary invoked only after the terminal transaction commits.
pub trait TerminalPublisher {
    /// Publishes one committed terminal and its exact fact positions.
    fn publish_terminal(
        &mut self,
        report: &ExecutionReport,
        positions: &[u64],
    ) -> Result<(), TerminalPublicationError>;
}

/// Durable report and publication disposition for one Core invocation.
pub struct DurableExecutionResult {
    /// Core report already mapped to durable terminal facts.
    pub report: ExecutionReport,
    /// Atomic execution/Turn terminal commit.
    pub terminal_commit: CommitResult,
    /// Publication failure leaves the committed terminal authoritative.
    pub publication: Result<(), TerminalPublicationError>,
}

impl fmt::Display for DurableExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(formatter, "durable execution command failed: {error}"),
            Self::Ledger(error) => write!(formatter, "durable execution ledger failed: {error}"),
            Self::Lease(error) => write!(formatter, "durable execution lease failed: {error}"),
            Self::Coordination => formatter.write_str("durable execution coordination failed"),
        }
    }
}

impl Error for DurableExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Ledger(error) => Some(error),
            Self::Lease(error) => Some(error),
            Self::Coordination => None,
        }
    }
}
