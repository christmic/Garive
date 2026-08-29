//! Internal wire vocabulary shared by the Messages codecs.

pub(crate) const METHOD_POST: &str = "POST";
pub(crate) const HEADER_CONTENT_TYPE: &str = "content-type";
pub(crate) const HEADER_ACCEPT: &str = "accept";
pub(crate) const MEDIA_JSON: &str = "application/json";
pub(crate) const MEDIA_SSE: &str = "text/event-stream";

pub(crate) const FIELD_TYPE: &str = "type";
pub(crate) const FIELD_DELTA: &str = "delta";
pub(crate) const FIELD_CONTENT_BLOCK: &str = "content_block";
pub(crate) const FIELD_TEXT: &str = "text";
pub(crate) const FIELD_PARTIAL_JSON: &str = "partial_json";
pub(crate) const FIELD_THINKING: &str = "thinking";
pub(crate) const FIELD_SIGNATURE: &str = "signature";
pub(crate) const FIELD_CITATION: &str = "citation";

pub(crate) const KIND_ERROR: &str = "error";
pub(crate) const KIND_TEXT: &str = "text";
pub(crate) const KIND_THINKING: &str = "thinking";
pub(crate) const KIND_REDACTED_THINKING: &str = "redacted_thinking";
pub(crate) const KIND_TOOL_USE: &str = "tool_use";

pub(crate) const EVENT_MESSAGE_START: &str = "message_start";
pub(crate) const EVENT_CONTENT_BLOCK_START: &str = "content_block_start";
pub(crate) const EVENT_CONTENT_BLOCK_DELTA: &str = "content_block_delta";
pub(crate) const EVENT_CONTENT_BLOCK_STOP: &str = "content_block_stop";
pub(crate) const EVENT_MESSAGE_DELTA: &str = "message_delta";
pub(crate) const EVENT_MESSAGE_STOP: &str = "message_stop";
pub(crate) const EVENT_PING: &str = "ping";

pub(crate) const DELTA_TEXT: &str = "text_delta";
pub(crate) const DELTA_INPUT_JSON: &str = "input_json_delta";
pub(crate) const DELTA_THINKING: &str = "thinking_delta";
pub(crate) const DELTA_SIGNATURE: &str = "signature_delta";
pub(crate) const DELTA_CITATIONS: &str = "citations_delta";

pub(crate) const CREATE_FIELDS: &[&str] = &[
    "model",
    "max_tokens",
    "messages",
    "stream",
    "system",
    "stop_sequences",
    "temperature",
    "top_p",
    "top_k",
    "tools",
    "tool_choice",
    "output_config",
    "thinking",
    "metadata",
];
