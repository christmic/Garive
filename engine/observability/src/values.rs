use serde::Serialize;

/// Stable portable O0 validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentSignalErrorCode {
    /// Envelope, correlation, ordering, time, or value shape is invalid.
    InvalidSignal,
    /// Signal name is outside the v1 catalogue.
    UnknownSignal,
    /// Attribute key/type/value is not admitted by this signal schema.
    AttributeNotAllowed,
    /// Attribute or measurement count exceeds the v1 bound.
    AttributeLimitExceeded,
    /// Measurement name, unit, or evidence is invalid.
    MeasurementInvalid,
    /// Signal redaction class is weaker than its schema requires.
    RedactionViolation,
}
impl AgentSignalErrorCode {
    /// Returns the exact stable wire name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidSignal => "invalid_signal",
            Self::UnknownSignal => "unknown_signal",
            Self::AttributeNotAllowed => "attribute_not_allowed",
            Self::AttributeLimitExceeded => "attribute_limit_exceeded",
            Self::MeasurementInvalid => "measurement_invalid",
            Self::RedactionViolation => "redaction_violation",
        }
    }
}

/// Typed portable validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentSignalError {
    code: AgentSignalErrorCode,
}
impl AgentSignalError {
    pub(crate) const fn new(code: AgentSignalErrorCode) -> Self {
        Self { code }
    }
    /// Returns the stable failure classification.
    pub const fn code(self) -> AgentSignalErrorCode {
        self.code
    }
}

/// Diagnostic severity and queue priority.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Finest diagnostic detail.
    Trace,
    /// Debug diagnostic detail.
    Debug,
    /// Normal operational information.
    Info,
    /// Degraded but continuing operation.
    Warn,
    /// Failed operation or invariant.
    Error,
}

/// Source sensitivity classification; sinks may only narrow access.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionClass {
    /// Safe public operational metadata.
    Public,
    /// Correlated internal operational metadata.
    Operational,
    /// Explicitly governed sensitive correlation metadata.
    Restricted,
}

/// Optional trace and durable-domain correlation, never metric labels.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Correlation {
    /// W3C-width lowercase trace identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// W3C-width lowercase span identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Optional parent span identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    /// Optional Session correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional Turn correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Optional Execution correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    /// Optional model request correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_request_id: Option<String>,
    /// Optional tool invocation correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_invocation_id: Option<String>,
    /// Exact committed fact position for durability-derived signals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable_position: Option<u64>,
}

/// Bounded allowlisted attribute value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttributeValue {
    /// Bounded stable classification.
    String {
        /// Exact allowlisted wire value.
        value: String,
    },
    /// Boolean low-cardinality flag.
    Bool {
        /// Exact flag value.
        value: bool,
    },
    /// Checked signed integer when a future schema admits it.
    Integer {
        /// Exact integer value.
        value: i64,
    },
}

/// One canonically ordered allowlisted attribute.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Attribute {
    /// Stable catalogue key.
    pub name: String,
    /// Typed bounded value.
    pub value: AttributeValue,
}

/// Portable exact measurement unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementUnit {
    /// Dimensionless count.
    Count,
    /// UTF-8 or serialized byte count.
    Bytes,
    /// Elapsed milliseconds.
    Milliseconds,
    /// Model token count.
    Tokens,
    /// Ratio scaled to 10,000.
    BasisPoints,
}

/// Known non-negative value or explicitly unknown token evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MeasurementValue {
    /// Exact checked measurement.
    Known {
        /// Non-negative value.
        value: u64,
    },
    /// No trustworthy token evidence exists.
    Unknown,
}

/// One canonically ordered catalogue measurement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Measurement {
    /// Stable catalogue name.
    pub name: String,
    /// Known or unknown evidence.
    pub value: MeasurementValue,
    /// Explicit unit.
    pub unit: MeasurementUnit,
}
