//! Resident terminal client for the Garive Host API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod args;

pub use args::{parse_launch_config, LaunchConfig, LaunchParseError, MouseMode, Theme};
