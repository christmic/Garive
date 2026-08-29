//! Provider-independent OpenAI Responses-compatible protocol adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod error;
mod events;
mod items;
mod request;
mod response;
mod sse;

pub use config::{Header, HttpRequest, ResponsesAdapter, ResponsesAdapterConfig};
pub use error::ResponsesAdapterError;
pub use events::{PortableEventKind, ResponseStreamEvent};
pub use items::{
    AssistantRole, OutputContent, OutputRefusal, OutputText, ProtocolExtension, ReasoningPart,
    ResponseFunctionCall, ResponseMessage, ResponseOutputItem, ResponseReasoning,
};
pub use request::{
    CreateResponseRequest, FunctionCallOutput, FunctionChoiceType, FunctionTool, ImageDetail,
    InputContent, InputItem, ItemStatus, MessageRole, ReasoningConfig, ResponseInput,
    ResponseTextConfig, ResponseTool, StreamOptions, TextFormat, ToolChoice, ToolChoiceMode,
    Truncation,
};
pub use response::{
    ApiError, DecodedResponse, ErrorEnvelope, IncompleteDetails, InputTokenDetails,
    OutputTokenDetails, Response, ResponseError, ResponseObject, ResponseStatus, ResponseUsage,
};
pub use sse::{SseDecoder, SseFrame};
