use std::num::NonZeroU64;

use garive_anthropic_messages::{
    CreateMessageRequest, Message, OutputConfig, SystemPrompt, ThinkingConfig, Tool, ToolChoice,
};
use serde::{Deserialize, Serialize};

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
