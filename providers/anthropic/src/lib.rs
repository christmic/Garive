//! Official Anthropic Messages connection and exact error profile.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod constants;

use garive_anthropic_messages::{Header, MessagesAdapterConfig};
use garive_llm::{RejectionKind, UnavailableKind};
use garive_provider_compatible::{ErrorDisposition, ErrorSignature, ProtocolErrorPolicy};
use garive_provider_profile::{ConnectionInput, VendorProfileError};

/// Adapter configuration and exact P2-C error policy produced together.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnthropicProfile {
    /// Explicit protocol adapter construction values.
    pub adapter_config: MessagesAdapterConfig,
    /// Exact official error classification rules.
    pub error_policy: ProtocolErrorPolicy,
}

/// Builds the official API-key profile from Runtime-supplied values.
pub fn build_profile(input: &ConnectionInput) -> Result<AnthropicProfile, VendorProfileError> {
    let resolved = input.resolve(constants::DEFAULT_ENDPOINT, constants::RESERVED_HEADERS)?;
    let mut headers = resolved
        .extra_headers()
        .iter()
        .map(|header| Header::new(header.name(), header.value(), header.is_sensitive()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VendorProfileError::ProfileInvariant)?;
    headers.push(
        Header::new(
            constants::API_KEY,
            resolved.credential().expose_secret(),
            true,
        )
        .map_err(|_| VendorProfileError::ProfileInvariant)?,
    );
    let adapter_config = MessagesAdapterConfig::new(
        resolved.endpoint(),
        headers,
        constants::VERSION_HEADER,
        constants::PROTOCOL_VERSION,
    )
    .map_err(|_| VendorProfileError::ProfileInvariant)?;
    Ok(AnthropicProfile {
        adapter_config,
        error_policy: default_error_policy()?,
    })
}

/// Returns the pinned exact official error policy.
pub fn default_error_policy() -> Result<ProtocolErrorPolicy, VendorProfileError> {
    ProtocolErrorPolicy::new([
        rule(
            401,
            "authentication_error",
            ErrorDisposition::Rejected(RejectionKind::Authentication),
        ),
        rule(
            429,
            "rate_limit_error",
            ErrorDisposition::Unavailable(UnavailableKind::RateLimited),
        ),
        rule(
            529,
            "overloaded_error",
            ErrorDisposition::Unavailable(UnavailableKind::ModelUnavailable),
        ),
    ])
    .map_err(|_| VendorProfileError::ProfileInvariant)
}

fn rule(
    status: u16,
    protocol_type: &str,
    disposition: ErrorDisposition,
) -> (ErrorSignature, ErrorDisposition) {
    (
        ErrorSignature {
            status,
            protocol_type: protocol_type.into(),
            code: None,
        },
        disposition,
    )
}
