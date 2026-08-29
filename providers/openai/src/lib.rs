//! Official OpenAI Responses connection and exact error profile.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod constants;

use garive_llm::{RejectionKind, UnavailableKind};
use garive_openai_responses::{Header, ResponsesAdapterConfig};
use garive_provider_compatible::{ErrorDisposition, ErrorSignature, ProtocolErrorPolicy};
use garive_provider_profile::{ConnectionInput, VendorProfileError};

/// Adapter configuration and exact P2-C error policy produced together.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiProfile {
    /// Explicit protocol adapter construction values.
    pub adapter_config: ResponsesAdapterConfig,
    /// Exact official error classification rules.
    pub error_policy: ProtocolErrorPolicy,
}

/// Builds the official profile from values supplied explicitly by Runtime.
pub fn build_profile(input: &ConnectionInput) -> Result<OpenAiProfile, VendorProfileError> {
    let resolved = input.resolve(constants::DEFAULT_ENDPOINT, constants::RESERVED_HEADERS)?;
    let mut headers = resolved
        .extra_headers()
        .iter()
        .map(|header| Header::new(header.name(), header.value(), header.is_sensitive()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VendorProfileError::ProfileInvariant)?;
    headers.push(
        Header::new(
            constants::AUTHORIZATION,
            format!("Bearer {}", resolved.credential().expose_secret()),
            true,
        )
        .map_err(|_| VendorProfileError::ProfileInvariant)?,
    );
    let adapter_config = ResponsesAdapterConfig::new(resolved.endpoint(), headers)
        .map_err(|_| VendorProfileError::ProfileInvariant)?;
    Ok(OpenAiProfile {
        adapter_config,
        error_policy: default_error_policy()?,
    })
}

/// Returns the pinned exact official error policy.
pub fn default_error_policy() -> Result<ProtocolErrorPolicy, VendorProfileError> {
    ProtocolErrorPolicy::new([
        rule(
            400,
            "invalid_request_error",
            "context_length_exceeded",
            ErrorDisposition::Rejected(RejectionKind::ContextOverflow),
        ),
        rule(
            401,
            "invalid_request_error",
            "invalid_api_key",
            ErrorDisposition::Rejected(RejectionKind::Authentication),
        ),
        rule(
            429,
            "rate_limit_error",
            "rate_limit_exceeded",
            ErrorDisposition::Unavailable(UnavailableKind::RateLimited),
        ),
        rule(
            503,
            "server_error",
            "server_error",
            ErrorDisposition::Unavailable(UnavailableKind::ModelUnavailable),
        ),
    ])
    .map_err(|_| VendorProfileError::ProfileInvariant)
}

fn rule(
    status: u16,
    protocol_type: &str,
    code: &str,
    disposition: ErrorDisposition,
) -> (ErrorSignature, ErrorDisposition) {
    (
        ErrorSignature {
            status,
            protocol_type: protocol_type.into(),
            code: Some(code.into()),
        },
        disposition,
    )
}
