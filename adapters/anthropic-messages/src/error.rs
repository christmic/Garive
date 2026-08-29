//! Stable protocol failures without deployment policy or secrets.

use std::fmt;

/// Failures raised while configuring or decoding the Messages protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessagesAdapterError {
    /// The configured endpoint is not an absolute HTTP(S) URI.
    InvalidEndpoint,
    /// A configured header name or value is invalid, duplicated, or reserved.
    InvalidHeader,
    /// The configured protocol version is empty.
    InvalidProtocolVersion,
    /// A typed request violates the portable protocol profile.
    InvalidRequest(&'static str),
    /// A JSON payload is malformed or has an invalid protocol shape.
    InvalidJson,
    /// The response media type is incompatible with the selected mode.
    InvalidMediaType,
    /// An SSE frame is malformed.
    InvalidSse,
    /// Stream events violate the Messages lifecycle.
    InvalidLifecycle(&'static str),
    /// EOF arrived before the current streaming response completed.
    TruncatedStream,
}

impl fmt::Display for MessagesAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidEndpoint => "Messages endpoint must be an absolute HTTP(S) URI",
            Self::InvalidHeader => "Messages adapter header is invalid or reserved",
            Self::InvalidProtocolVersion => "Messages protocol version must not be empty",
            Self::InvalidRequest(reason) => reason,
            Self::InvalidJson => "Messages payload is not valid protocol JSON",
            Self::InvalidMediaType => "Messages payload has an incompatible media type",
            Self::InvalidSse => "Messages stream contains an invalid SSE frame",
            Self::InvalidLifecycle(reason) => reason,
            Self::TruncatedStream => "Messages stream ended before its protocol terminal",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for MessagesAdapterError {}
