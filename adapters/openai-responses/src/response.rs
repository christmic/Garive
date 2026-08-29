//! Ordinary response, usage, and protocol error envelopes.

use crate::{Header, ResponseOutputItem, ResponsesAdapter, ResponsesAdapterError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Official core response envelope with lossless optional extensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// Unique response identifier.
    pub id: String,
    /// Unix timestamp in seconds.
    pub created_at: f64,
    /// Optional generation failure details.
    pub error: Option<ResponseError>,
    /// Optional incomplete-response details.
    pub incomplete_details: Option<IncompleteDetails>,
    /// Optional instructions returned by the protocol.
    pub instructions: Option<Value>,
    /// Optional response metadata.
    pub metadata: Option<BTreeMap<String, String>>,
    /// Model identifier reported by the endpoint.
    pub model: String,
    /// Fixed object discriminator.
    pub object: ResponseObject,
    /// Ordered output items.
    pub output: Vec<ResponseOutputItem>,
    /// Whether parallel function calls were enabled.
    pub parallel_tool_calls: bool,
    /// Sampling temperature returned by the endpoint.
    pub temperature: Option<f64>,
    /// Text output configuration returned by the endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Value>,
    /// Lossless tool-choice value.
    pub tool_choice: Value,
    /// Lossless tool definitions returned by the endpoint.
    pub tools: Vec<Value>,
    /// Nucleus sampling probability returned by the endpoint.
    pub top_p: Option<f64>,
    /// Optional lifecycle status.
    pub status: Option<ResponseStatus>,
    /// Optional protocol usage.
    pub usage: Option<ResponseUsage>,
    /// Non-colliding optional or future response fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

impl Response {
    /// Validates required values and official usage arithmetic.
    pub fn validate(&self) -> Result<(), ResponsesAdapterError> {
        if self.id.is_empty()
            || self.model.is_empty()
            || !self.created_at.is_finite()
            || self.created_at < 0.0
        {
            return Err(ResponsesAdapterError::InvalidJson);
        }
        if let Some(usage) = &self.usage {
            usage.validate()?;
        }
        if self.temperature.is_some_and(|value| !value.is_finite())
            || self.top_p.is_some_and(|value| !value.is_finite())
        {
            return Err(ResponsesAdapterError::InvalidJson);
        }
        if matches!(self.status, Some(ResponseStatus::Completed)) && self.error.is_some() {
            return Err(ResponsesAdapterError::InvalidJson);
        }
        Ok(())
    }
}

/// Fixed `response` object discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseObject {
    /// Responses API object.
    Response,
}

/// Official response lifecycle states.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    /// Work is queued.
    Queued,
    /// Work is executing.
    InProgress,
    /// Work completed successfully.
    Completed,
    /// Work failed.
    Failed,
    /// Work was cancelled.
    Cancelled,
    /// Work stopped before completion.
    Incomplete,
}

/// Error attached to a response object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponseError {
    /// Protocol error code.
    pub code: String,
    /// Human-readable protocol message.
    pub message: String,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Reason attached to an incomplete response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IncompleteDetails {
    /// Protocol reason string.
    pub reason: String,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Official response usage fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResponseUsage {
    /// Input token count.
    pub input_tokens: u64,
    /// Input token breakdown.
    pub input_tokens_details: InputTokenDetails,
    /// Output token count.
    pub output_tokens: u64,
    /// Output token breakdown.
    pub output_tokens_details: OutputTokenDetails,
    /// Checked total token count reported by the protocol.
    pub total_tokens: u64,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

impl ResponseUsage {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        if self.input_tokens.checked_add(self.output_tokens) != Some(self.total_tokens) {
            return Err(ResponsesAdapterError::InvalidJson);
        }
        Ok(())
    }
}

/// Official input token detail fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputTokenDetails {
    /// Tokens read from a cache.
    pub cached_tokens: u64,
    /// Tokens written to a cache.
    pub cache_write_tokens: u64,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Official output token detail fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputTokenDetails {
    /// Reasoning token count.
    pub reasoning_tokens: u64,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Standard non-success error object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    /// Optional machine-readable error code.
    pub code: Option<String>,
    /// Human-readable protocol message.
    pub message: String,
    /// Optional field associated with the error.
    pub param: Option<String>,
    /// Protocol error type.
    pub r#type: String,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Standard outer error envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    /// Structured protocol error.
    pub error: ApiError,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// One decoded ordinary HTTP exchange without Provider policy.
#[derive(Clone, Debug, PartialEq)]
pub enum DecodedResponse {
    /// A successful ordinary response.
    Response {
        /// HTTP status supplied by Runtime transport.
        status: u16,
        /// Response headers supplied by Runtime transport.
        headers: Vec<Header>,
        /// Typed protocol response.
        response: Box<Response>,
    },
    /// A non-success protocol error response.
    Error {
        /// HTTP status supplied by Runtime transport.
        status: u16,
        /// Response headers supplied by Runtime transport.
        headers: Vec<Header>,
        /// Typed protocol error envelope.
        error: ErrorEnvelope,
    },
}

impl ResponsesAdapter {
    /// Decodes one buffered JSON response without classifying Provider policy.
    pub fn decode_response(
        &self,
        status: u16,
        headers: &[Header],
        body: &[u8],
    ) -> Result<DecodedResponse, ResponsesAdapterError> {
        require_json_media(headers)?;
        if (200..300).contains(&status) {
            let response: Response =
                serde_json::from_slice(body).map_err(|_| ResponsesAdapterError::InvalidJson)?;
            response.validate()?;
            Ok(DecodedResponse::Response {
                status,
                headers: headers.to_vec(),
                response: Box::new(response),
            })
        } else {
            let error =
                serde_json::from_slice(body).map_err(|_| ResponsesAdapterError::InvalidJson)?;
            Ok(DecodedResponse::Error {
                status,
                headers: headers.to_vec(),
                error,
            })
        }
    }
}

fn require_json_media(headers: &[Header]) -> Result<(), ResponsesAdapterError> {
    let media = headers
        .iter()
        .find(|header| header.name() == "content-type")
        .map(Header::value)
        .unwrap_or("application/json");
    if media.split(';').next() == Some("application/json") {
        Ok(())
    } else {
        Err(ResponsesAdapterError::InvalidMediaType)
    }
}
