//! Explicit managed-browser endpoint and resource limits.

use std::{error::Error, fmt, net::IpAddr};

use url::{Host, Url};

/// Bounded transport and correlation limits supplied by Runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CdpLimits {
    /// Maximum one-frame UTF-8 payload size.
    pub max_frame_bytes: usize,
    /// Maximum commands awaiting a response; v1 requires exactly one.
    pub max_in_flight: usize,
    /// Maximum queued unsolicited events.
    pub max_queued_events: usize,
    /// Maximum wall-clock duration of connect or one command exchange.
    pub operation_timeout_ms: u64,
}

impl CdpLimits {
    /// Validates explicit non-zero hard ceilings.
    pub fn new(
        max_frame_bytes: usize,
        max_in_flight: usize,
        max_queued_events: usize,
        operation_timeout_ms: u64,
    ) -> Result<Self, CdpAdapterConfigError> {
        let value = Self {
            max_frame_bytes,
            max_in_flight,
            max_queued_events,
            operation_timeout_ms,
        };
        if max_frame_bytes == 0
            || max_frame_bytes > 16_777_216
            || max_in_flight != 1
            || max_queued_events == 0
            || max_queued_events > 10_000
            || operation_timeout_ms == 0
            || operation_timeout_ms > 120_000
        {
            Err(CdpAdapterConfigError::InvalidLimits)
        } else {
            Ok(value)
        }
    }
}

/// Immutable CDP construction inputs; no environment discovery is allowed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpAdapterConfig {
    endpoint: Url,
    limits: CdpLimits,
}

impl CdpAdapterConfig {
    /// Admits only an explicit loopback managed-browser WebSocket endpoint.
    pub fn new(
        endpoint: impl AsRef<str>,
        limits: CdpLimits,
    ) -> Result<Self, CdpAdapterConfigError> {
        let endpoint =
            Url::parse(endpoint.as_ref()).map_err(|_| CdpAdapterConfigError::InvalidEndpoint)?;
        let loopback = match endpoint.host() {
            Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
            Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
            Some(Host::Domain("localhost")) => true,
            _ => false,
        };
        if endpoint.scheme() != "ws"
            || !loopback
            || endpoint.port().is_none()
            || endpoint.username() != ""
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(CdpAdapterConfigError::InvalidEndpoint);
        }
        Ok(Self { endpoint, limits })
    }

    /// Returns the exact managed-browser endpoint.
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// Returns the frozen resource limits.
    pub const fn limits(&self) -> CdpLimits {
        self.limits
    }
}

/// Stable explicit construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdpAdapterConfigError {
    /// Endpoint is not an explicit loopback WebSocket with a port.
    InvalidEndpoint,
    /// One or more resource limits are zero or above the hard ceiling.
    InvalidLimits,
}

impl fmt::Display for CdpAdapterConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEndpoint => "invalid managed CDP endpoint",
            Self::InvalidLimits => "invalid CDP resource limits",
        })
    }
}

impl Error for CdpAdapterConfigError {}
