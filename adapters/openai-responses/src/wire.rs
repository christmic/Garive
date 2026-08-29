//! Internal wire vocabulary shared by the Responses codecs.

pub(crate) const METHOD_POST: &str = "POST";
pub(crate) const HEADER_CONTENT_TYPE: &str = "content-type";
pub(crate) const HEADER_ACCEPT: &str = "accept";
pub(crate) const MEDIA_JSON: &str = "application/json";
pub(crate) const MEDIA_SSE: &str = "text/event-stream";

pub(crate) const FIELD_TYPE: &str = "type";
pub(crate) const FIELD_RESPONSE: &str = "response";
pub(crate) const FIELD_SEQUENCE_NUMBER: &str = "sequence_number";

pub(crate) const KIND_MESSAGE: &str = "message";
pub(crate) const KIND_FUNCTION_CALL: &str = "function_call";
pub(crate) const KIND_REASONING: &str = "reasoning";
pub(crate) const KIND_OUTPUT_TEXT: &str = "output_text";
pub(crate) const KIND_REFUSAL: &str = "refusal";
pub(crate) const CREATE_FIELDS: &[&str] = &[
    "model",
    "input",
    "max_output_tokens",
    "temperature",
    "top_p",
    "truncation",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "text",
    "reasoning",
    "metadata",
    "stream",
    "stream_options",
];
