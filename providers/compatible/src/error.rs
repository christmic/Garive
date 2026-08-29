use garive_llm::{ModelCapability, RequestValidationError};

/// Stable failure produced before protocol transport is invoked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatibleProviderError {
    /// Neutral request validation failed.
    InvalidRequest(RequestValidationError),
    /// Request selected a different immutable target.
    TargetMismatch,
    /// Deployment does not admit a required capability.
    UnsupportedCapability(ModelCapability),
    /// A JSON schema was not a JSON object.
    InvalidJsonObject,
    /// The protocol cannot represent this neutral input.
    UnsupportedInput,
    /// Messages-compatible portable mapping does not admit trace metadata.
    UnsupportedMetadata,
    /// A neutral media reference has no explicit deployment binding.
    MissingMediaBinding,
    /// Messages-compatible request has no explicit or configured output limit.
    MissingOutputLimit,
    /// Adapter validation rejected the mapped protocol request.
    InvalidProtocolRequest,
    /// An unadmitted protocol extension was observed.
    UnsupportedExtension,
    /// No exact deployment rule classified a protocol error.
    UnclassifiedProtocolError,
    /// A protocol terminal contradicted the portable lifecycle contract.
    ProtocolInvariant,
}

impl CompatibleProviderError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::TargetMismatch => "target_mismatch",
            Self::UnsupportedCapability(_) => "unsupported_capability",
            Self::InvalidJsonObject => "invalid_json_object",
            Self::UnsupportedInput => "unsupported_input",
            Self::UnsupportedMetadata => "unsupported_metadata",
            Self::MissingMediaBinding => "missing_media_binding",
            Self::MissingOutputLimit => "missing_output_limit",
            Self::InvalidProtocolRequest => "invalid_protocol_request",
            Self::UnsupportedExtension => "unsupported_extension",
            Self::UnclassifiedProtocolError => "unclassified_protocol_error",
            Self::ProtocolInvariant => "protocol_invariant",
        }
    }
}
