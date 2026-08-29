use std::{fmt, num::NonZeroU64};

use garive_anthropic_messages::{
    CreateMessageRequest, Header, Message, OutputConfig, SystemPrompt, ThinkingConfig, Tool,
    ToolChoice,
};
use garive_provider_profile::{ConnectionInput, VendorProfileError};
use serde::{Deserialize, Serialize};

use crate::constants;

/// Stable failure for the exact token-count capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnthropicTokenCountError {
    /// The portable create request failed its native validation.
    InvalidRequest,
    /// The create request contains provider extensions not admitted for counting.
    UnsupportedExtension,
    /// The success body is not the exact positive token-count shape.
    InvalidResponse,
}

impl AnthropicTokenCountError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::UnsupportedExtension => "unsupported_extension",
            Self::InvalidResponse => "invalid_response",
        }
    }
}

/// Exact official request projection for `POST /v1/messages/count_tokens`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CountTokensRequest {
    /// Deployment-selected model identifier.
    pub model: String,
    /// Ordered user and assistant turns.
    pub messages: Vec<Message>,
    /// Optional top-level system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    /// Client-executed tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    /// Tool selection policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Structured output and effort configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// Extended-thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

/// Explicit official connection profile for one token-count exchange.
#[derive(Clone, Eq, PartialEq)]
pub struct AnthropicTokenCountProfile {
    endpoint: String,
    headers: Vec<Header>,
}

impl AnthropicTokenCountProfile {
    /// Returns the immutable endpoint selected by Runtime configuration.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the ordered explicit and protocol-required headers.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// Encodes one already projected request without executing HTTP.
    pub fn prepare(
        &self,
        request: &CountTokensRequest,
    ) -> Result<TokenCountHttpRequest, AnthropicTokenCountError> {
        let body =
            serde_json::to_vec(request).map_err(|_| AnthropicTokenCountError::InvalidRequest)?;
        Ok(TokenCountHttpRequest {
            uri: self.endpoint.clone(),
            headers: self.headers.clone(),
            body,
        })
    }
}

impl fmt::Debug for AnthropicTokenCountProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicTokenCountProfile")
            .field("endpoint", &self.endpoint)
            .field("headers", &self.headers)
            .finish()
    }
}

/// Builds the vendor capability only from explicit Runtime-supplied values.
pub fn build_token_count_profile(
    input: &ConnectionInput,
) -> Result<AnthropicTokenCountProfile, VendorProfileError> {
    let resolved = input.resolve(
        constants::TOKEN_COUNT_DEFAULT_ENDPOINT,
        constants::RESERVED_HEADERS,
    )?;
    let mut headers = resolved
        .extra_headers()
        .iter()
        .map(|header| Header::new(header.name(), header.value(), header.is_sensitive()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VendorProfileError::ProfileInvariant)?;
    for (name, value, sensitive) in [
        (
            constants::API_KEY,
            resolved.credential().expose_secret(),
            true,
        ),
        (
            constants::VERSION_HEADER,
            constants::PROTOCOL_VERSION,
            false,
        ),
        (constants::CONTENT_TYPE, constants::MEDIA_JSON, false),
        (constants::ACCEPT, constants::MEDIA_JSON, false),
    ] {
        headers.push(
            Header::new(name, value, sensitive)
                .map_err(|_| VendorProfileError::ProfileInvariant)?,
        );
    }
    Ok(AnthropicTokenCountProfile {
        endpoint: resolved.endpoint().to_owned(),
        headers,
    })
}

/// Fully described vendor token-count HTTP request for Runtime transport.
#[derive(Clone, Eq, PartialEq)]
pub struct TokenCountHttpRequest {
    uri: String,
    headers: Vec<Header>,
    body: Vec<u8>,
}

impl TokenCountHttpRequest {
    /// Returns the required HTTP method.
    pub const fn method(&self) -> &'static str {
        constants::METHOD_POST
    }

    /// Returns the explicit absolute endpoint.
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Returns the ordered headers, including redacted sensitive values.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// Returns the exact encoded request body.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

impl fmt::Debug for TokenCountHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenCountHttpRequest")
            .field("uri", &self.uri)
            .field("headers", &self.headers)
            .field("body_length", &self.body.len())
            .finish()
    }
}

/// Projects one validated portable create request without generation-only fields.
pub fn project_token_count_request(
    request: &CreateMessageRequest,
) -> Result<CountTokensRequest, AnthropicTokenCountError> {
    request
        .validate()
        .map_err(|_| AnthropicTokenCountError::InvalidRequest)?;
    if !request.extensions.is_empty() {
        return Err(AnthropicTokenCountError::UnsupportedExtension);
    }
    Ok(CountTokensRequest {
        model: request.model.clone(),
        messages: request.messages.clone(),
        system: request.system.clone(),
        tools: request.tools.clone(),
        tool_choice: request.tool_choice.clone(),
        output_config: request.output_config.clone(),
        thinking: request.thinking.clone(),
    })
}

/// Positive exact input-token count returned by the vendor capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenCount(NonZeroU64);

impl TokenCount {
    /// Returns the exact provider-reported input token count.
    pub const fn input_tokens(self) -> u64 {
        self.0.get()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenCountWire {
    input_tokens: u64,
}

/// Decodes one exact successful token-count response body.
pub fn decode_token_count(body: &[u8]) -> Result<TokenCount, AnthropicTokenCountError> {
    let response: TokenCountWire =
        serde_json::from_slice(body).map_err(|_| AnthropicTokenCountError::InvalidResponse)?;
    NonZeroU64::new(response.input_tokens)
        .map(TokenCount)
        .ok_or(AnthropicTokenCountError::InvalidResponse)
}
