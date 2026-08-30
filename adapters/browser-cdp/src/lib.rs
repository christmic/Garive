//! Runtime-independent Chrome DevTools Protocol wire adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;
mod config;
mod transport;
mod wire;

pub use config::{CdpAdapterConfig, CdpAdapterConfigError, CdpLimits};
pub use transport::{CdpTransport, CdpTransportError};
pub use wire::{parse_incoming, CdpCommand, CdpError, CdpIncoming, CdpProtocolError};

/// Frozen adapter implementation revision.
pub const CDP_ADAPTER_REVISION: &str = "garive.browser.cdp.v1";
pub use client::{CdpAxNode, CdpAxProperty, CdpAxTree, CdpBrowserVersion, CdpClient};
