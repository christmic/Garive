//! Stable protocol adapter failures without deployment data or secrets.

use std::fmt;

/// Failures raised while configuring or decoding the Responses protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponsesAdapterError {
    /// The configured endpoint is not an absolute HTTP(S) URI.
    InvalidEndpoint,
    /// A configured header name or value is invalid or reserved.
    InvalidHeader,
    /// A typed request violates the portable protocol profile.
    InvalidRequest(&'static str),
    /// A JSON payload is malformed or has an invalid protocol shape.
    InvalidJson,
    /// The response media type is incompatible with the selected mode.
    InvalidMediaType,
    /// An SSE frame is malformed.
    InvalidSse,
    /// Stream events violate the Responses lifecycle.
    InvalidLifecycle(&'static str),
    /// EOF arrived before the current SSE response completed.
    TruncatedStream,
}

impl fmt::Display for ResponsesAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidEndpoint => "Responses endpoint must be an absolute HTTP(S) URI",
            Self::InvalidHeader => "Responses adapter header is invalid or reserved",
            Self::InvalidRequest(reason) => reason,
            Self::InvalidJson => "Responses payload is not valid protocol JSON",
            Self::InvalidMediaType => "Responses payload has an incompatible media type",
            Self::InvalidSse => "Responses stream contains an invalid SSE frame",
            Self::InvalidLifecycle(reason) => reason,
            Self::TruncatedStream => "Responses stream ended before its protocol terminal",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ResponsesAdapterError {}
