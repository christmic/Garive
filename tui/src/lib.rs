//! Resident terminal client for the Garive Host API.

#![deny(unsafe_code)]
#![deny(missing_docs)]

mod application;
mod args;
mod input;
mod persistence;
mod runtime;
mod view;

pub use args::{parse_launch_config, LaunchConfig, LaunchParseError, MouseMode, Theme};
pub use runtime::run;

/// Safe top-level failure returned by the resident terminal application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiError {
    /// Standard input or error is not an interactive terminal.
    TerminalUnavailable,
    /// Terminal setup, rendering, input, or restoration failed.
    TerminalIo,
    /// The configured Host endpoint is invalid.
    InvalidHost,
    /// Local presentation or recovery state could not be safely opened.
    LocalState,
    /// The process received an operating-system termination signal after setup.
    Interrupted(i32),
}

impl std::fmt::Display for TuiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::TerminalUnavailable => "an interactive terminal is required",
            Self::TerminalIo => "terminal operation failed",
            Self::InvalidHost => "invalid Host configuration",
            Self::LocalState => "local state is unavailable or unsafe",
            Self::Interrupted(_) => "interrupted after terminal restoration",
        })
    }
}

impl std::error::Error for TuiError {}
