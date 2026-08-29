//! Ordinary Messages response, usage, and protocol error envelopes.

use crate::{Header, MessagesAdapter, MessagesAdapterError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Official core message envelope with lossless optional extensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageResponse {
    /// Unique message identifier.
    pub id: String,
    /// Fixed object discriminator.
    pub r#type: ResponseType,
    /// Fixed assistant role.
    pub role: ResponseRole,
    /// Model identifier reported by the endpoint.
    pub model: String,
    /// Ordered output content.
    pub content: Vec<OutputBlock>,
    /// Optional stop reason; null in `message_start`.
    pub stop_reason: Option<StopReason>,
    /// Matched custom stop sequence.
    pub stop_sequence: Option<String>,
    /// Structured refusal or future stop data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<Value>,
    /// Protocol usage exactly as reported.
    pub usage: Usage,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

impl MessageResponse {
    /// Validates required identifiers and output blocks.
    pub fn validate(&self) -> Result<(), MessagesAdapterError> {
        if self.id.is_empty() || self.model.is_empty() {
            return Err(MessagesAdapterError::InvalidJson);
        }
        self.content.iter().try_for_each(OutputBlock::validate)
    }
}

/// Fixed `message` discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    /// Message response object.
    Message,
}

/// Fixed response role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseRole {
    /// Assistant output.
    Assistant,
}

/// Official portable stop reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural end of turn.
    EndTurn,
    /// Output token limit reached.
    MaxTokens,
    /// Custom stop sequence reached.
    StopSequence,
    /// Client tool invocation emitted.
    ToolUse,
    /// Long-running turn paused.
    PauseTurn,
    /// Safety refusal.
    Refusal,
    /// Model context window exceeded.
    ModelContextWindowExceeded,
}

/// Portable output blocks plus a lossless future/hosted extension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputBlock {
    /// Text with lossless citation objects.
    Text(OutputText),
    /// Extended thinking with integrity signature.
    Thinking(OutputThinking),
    /// Opaque safety-redacted thinking.
    RedactedThinking(OutputRedactedThinking),
    /// Client tool invocation.
    ToolUse(OutputToolUse),
    /// Hosted or future output block retained without promotion.
    Extension(Map<String, Value>),
}

impl OutputBlock {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        let invalid = match self {
            Self::Text(value) => value.text.is_empty(),
            Self::Thinking(value) => value.thinking.is_empty() || value.signature.is_empty(),
            Self::RedactedThinking(value) => value.data.is_empty(),
            Self::ToolUse(value) => value.id.is_empty() || value.name.is_empty(),
            Self::Extension(value) => {
                !matches!(value.get("type"), Some(Value::String(kind)) if !kind.is_empty())
            }
        };
        if invalid {
            Err(MessagesAdapterError::InvalidJson)
        } else {
            Ok(())
        }
    }
}

/// Official text output block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputText {
    /// Fixed discriminator.
    pub r#type: OutputTextType,
    /// Generated text.
    pub text: String,
    /// Lossless citation objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Value>>,
}

/// Text discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputTextType {
    /// Text output.
    Text,
}

/// Official thinking output block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputThinking {
    /// Fixed discriminator.
    pub r#type: OutputThinkingType,
    /// Thinking text.
    pub thinking: String,
    /// Opaque integrity signature.
    pub signature: String,
}

/// Thinking discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputThinkingType {
    /// Thinking output.
    Thinking,
}

/// Official redacted-thinking output block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputRedactedThinking {
    /// Fixed discriminator.
    pub r#type: OutputRedactedThinkingType,
    /// Opaque encrypted data.
    pub data: String,
}

/// Redacted-thinking discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputRedactedThinkingType {
    /// Redacted thinking output.
    RedactedThinking,
}

/// Official client tool-use output block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputToolUse {
    /// Fixed discriminator.
    pub r#type: OutputToolUseType,
    /// Invocation identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// JSON input object.
    pub input: Map<String, Value>,
    /// Optional caller detail retained losslessly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
}

/// Tool-use discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputToolUseType {
    /// Client tool invocation.
    ToolUse,
}

/// Official usage fields retained without derived totals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Uncached input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Cache-creation input tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    /// Cache-read input tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    /// Breakdown of cache writes by TTL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<CacheCreation>,
    /// Geographic region that performed inference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    /// Output-token observability breakdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<OutputTokensDetails>,
    /// Hosted server-tool request counters retained as wire data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<ServerToolUsage>,
    /// Service tier reported by the endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    /// Non-colliding future usage data.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Official cache-creation breakdown by TTL.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheCreation {
    /// Input tokens written to a one-hour cache entry.
    pub ephemeral_1h_input_tokens: u64,
    /// Input tokens written to a five-minute cache entry.
    pub ephemeral_5m_input_tokens: u64,
}

/// Official output-token detail fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputTokensDetails {
    /// Tokens spent on internal reasoning.
    pub thinking_tokens: u64,
}

/// Official server-tool usage counters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerToolUsage {
    /// Web-fetch requests.
    pub web_fetch_requests: u64,
    /// Web-search requests.
    pub web_search_requests: u64,
}

/// Service tier reported in response usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    /// Standard service.
    Standard,
    /// Priority service.
    Priority,
    /// Batch service.
    Batch,
}

/// Protocol error object with an open type string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    /// Open protocol error type.
    pub r#type: String,
    /// Human-readable protocol message.
    pub message: String,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Standard outer error envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    /// Fixed outer discriminator retained as text for compatibility.
    pub r#type: String,
    /// Structured protocol error.
    pub error: ApiError,
    /// Optional protocol request identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// One decoded ordinary HTTP exchange without Provider policy.
#[derive(Clone, Debug, PartialEq)]
pub enum DecodedResponse {
    /// Successful ordinary message.
    Message {
        /// HTTP status.
        status: u16,
        /// Response headers.
        headers: Vec<Header>,
        /// Typed message.
        message: Box<MessageResponse>,
    },
    /// Non-success protocol error.
    Error {
        /// HTTP status.
        status: u16,
        /// Response headers.
        headers: Vec<Header>,
        /// Typed error.
        error: ErrorEnvelope,
    },
}

impl MessagesAdapter {
    /// Decodes buffered JSON while preserving status and headers as protocol facts.
    pub fn decode_response(
        &self,
        status: u16,
        headers: &[Header],
        body: &[u8],
    ) -> Result<DecodedResponse, MessagesAdapterError> {
        require_json_media(headers)?;
        if (200..300).contains(&status) {
            let message: MessageResponse =
                serde_json::from_slice(body).map_err(|_| MessagesAdapterError::InvalidJson)?;
            message.validate()?;
            Ok(DecodedResponse::Message {
                status,
                headers: headers.to_vec(),
                message: Box::new(message),
            })
        } else {
            let error: ErrorEnvelope =
                serde_json::from_slice(body).map_err(|_| MessagesAdapterError::InvalidJson)?;
            if error.r#type != "error"
                || error.error.r#type.is_empty()
                || error.error.message.is_empty()
            {
                return Err(MessagesAdapterError::InvalidJson);
            }
            Ok(DecodedResponse::Error {
                status,
                headers: headers.to_vec(),
                error,
            })
        }
    }
}

fn require_json_media(headers: &[Header]) -> Result<(), MessagesAdapterError> {
    let media = headers
        .iter()
        .find(|header| header.name() == "content-type")
        .map(Header::value)
        .unwrap_or("application/json");
    if media.split(';').next() == Some("application/json") {
        Ok(())
    } else {
        Err(MessagesAdapterError::InvalidMediaType)
    }
}
