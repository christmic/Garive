//! Runtime-independent explicit-tab Browser attachment protocol.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod framing;

pub use config::{AttachedConfig, AttachedConfigError, AttachedLimits};
pub use framing::{read_frame, write_frame, AttachedFrameError};

/// Frozen Attached Browser protocol revision.
pub const ATTACHED_PROTOCOL_REVISION: &str = "garive.browser.attached.v1";
