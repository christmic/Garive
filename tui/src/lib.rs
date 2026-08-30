//! Resident terminal client for the Garive Host API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod application;
mod args;
mod input;
mod runtime;
mod view;

pub use args::{parse_launch_config, LaunchConfig, LaunchParseError, MouseMode, Theme};
