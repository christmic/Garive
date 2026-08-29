//! Provider composition for portable Responses- and Messages-compatible deployments.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod deployment;
mod error;
mod outcome;
mod request;

pub use deployment::{
    ErrorDisposition, ErrorSignature, MessagesDeployment, MessagesMediaBinding, PolicyBuildError,
    ProtocolErrorPolicy, ResponsesDeployment, ResponsesMediaBinding,
};
pub use error::CompatibleProviderError;
pub use outcome::{classify_protocol_error, normalize_messages, normalize_responses};
pub use request::{map_messages_request, map_responses_request};
