//! Resident terminal client for the Garive Host API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod application;
mod args;
mod input;
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
}

impl std::fmt::Display for TuiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::TerminalUnavailable => "an interactive terminal is required",
            Self::TerminalIo => "terminal operation failed",
            Self::InvalidHost => "invalid Host configuration",
        })
    }
}

impl std::error::Error for TuiError {}
