//! Runtime-independent Chrome DevTools Protocol wire adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod wire;

pub use config::{CdpAdapterConfig, CdpAdapterConfigError, CdpLimits};
pub use wire::{parse_incoming, CdpCommand, CdpError, CdpIncoming, CdpProtocolError};

/// Frozen adapter implementation revision.
pub const CDP_ADAPTER_REVISION: &str = "garive.browser.cdp.v1";
