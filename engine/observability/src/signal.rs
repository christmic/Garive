use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::catalogue::attribute_valid;
use crate::{
    signal_schema, AgentSignalError, AgentSignalErrorCode, Attribute, AttributeValue, Correlation,
    Measurement, MeasurementUnit, MeasurementValue, RedactionClass, Severity,
};

const MAX_ATTRIBUTES: usize = 8;
const MAX_MEASUREMENTS: usize = 8;
const MAX_STRING_BYTES: usize = 64;
const MAX_ID_BYTES: usize = 128;

/// Canonical serialized signal and its SHA-256 digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalBinding {
    /// SHA-256 of canonical bytes.
    pub digest: String,
    /// RFC 8785 canonical signal JSON.
    pub inline_utf8: String,
}

/// Validated immutable portable v1 Agent signal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentSignal {
    signal_name: String,
    schema_version: u32,
    observed_at_utc: String,
    severity: Severity,
    correlation: Correlation,
    attributes: Vec<Attribute>,
    measurements: Vec<Measurement>,
    redaction_class: RedactionClass,
}
impl AgentSignal {
    /// Validates catalogue, ordering, bounds, correlation, measurements and redaction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        signal_name: impl Into<String>,
        schema_version: u32,
        observed_at_utc: impl Into<String>,
        severity: Severity,
        correlation: Correlation,
        attributes: Vec<Attribute>,
        measurements: Vec<Measurement>,
        redaction_class: RedactionClass,
    ) -> Result<Self, AgentSignalError> {
        let value = Self {
            signal_name: signal_name.into(),
            schema_version,
            observed_at_utc: observed_at_utc.into(),
            severity,
            correlation,
            attributes,
            measurements,
            redaction_class,
        };
        value.validate()?;
        Ok(value)
    }
    /// Returns exact catalogue name.
    pub fn signal_name(&self) -> &str {
        &self.signal_name
    }
    /// Returns diagnostic severity and queue priority.
    pub const fn severity(&self) -> Severity {
        self.severity
    }
    /// Returns source sensitivity.
    pub const fn redaction_class(&self) -> RedactionClass {
        self.redaction_class
    }
    /// Returns immutable correlation fields.
    pub const fn correlation(&self) -> &Correlation {
        &self.correlation
    }
    /// Returns canonical attributes.
    pub fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
    /// Returns canonical measurements.
    pub fn measurements(&self) -> &[Measurement] {
        &self.measurements
    }
    /// Returns RFC 8785 canonical JSON and SHA-256.
    pub fn binding(&self) -> Result<SignalBinding, AgentSignalError> {
        let bytes =
            serde_jcs::to_vec(self).map_err(|_| error(AgentSignalErrorCode::InvalidSignal))?;
        let inline_utf8 =
            String::from_utf8(bytes).map_err(|_| error(AgentSignalErrorCode::InvalidSignal))?;
        Ok(SignalBinding {
            digest: format!("{:x}", Sha256::digest(inline_utf8.as_bytes())),
            inline_utf8,
        })
    }
    fn validate(&self) -> Result<(), AgentSignalError> {
        if self.schema_version != 1
            || !canonical_time(&self.observed_at_utc)
            || !valid_correlation(&self.correlation)
            || !strict_names(self.attributes.iter().map(|v| v.name.as_str()))
            || !strict_names(self.measurements.iter().map(|v| v.name.as_str()))
        {
            return Err(error(AgentSignalErrorCode::InvalidSignal));
        }
        if self.attributes.len() > MAX_ATTRIBUTES || self.measurements.len() > MAX_MEASUREMENTS {
            return Err(error(AgentSignalErrorCode::AttributeLimitExceeded));
        }
        let schema = signal_schema(&self.signal_name)
            .ok_or_else(|| error(AgentSignalErrorCode::UnknownSignal))?;
        if self.redaction_class < schema.minimum_redaction {
            return Err(error(AgentSignalErrorCode::RedactionViolation));
        }
        for attribute in &self.attributes {
            let Some((_, category)) = schema
                .attributes
                .iter()
                .find(|(name, _)| *name == attribute.name)
            else {
                return Err(error(AgentSignalErrorCode::AttributeNotAllowed));
            };
            if !attribute_valid(category, &attribute.value)
                || matches!(&attribute.value,AttributeValue::String{value} if !valid_text(value,MAX_STRING_BYTES))
            {
                return Err(error(AgentSignalErrorCode::AttributeNotAllowed));
            }
        }
        for measurement in &self.measurements {
            let Some((_, unit)) = schema
                .measurements
                .iter()
                .find(|(name, _)| *name == measurement.name)
            else {
                return Err(error(AgentSignalErrorCode::MeasurementInvalid));
            };
            if *unit != measurement.unit
                || matches!(measurement.value, MeasurementValue::Unknown)
                    && !(measurement.unit == MeasurementUnit::Tokens
                        && matches!(measurement.name.as_str(), "input_tokens" | "output_tokens"))
            {
                return Err(error(AgentSignalErrorCode::MeasurementInvalid));
            }
        }
        Ok(())
    }
}
fn strict_names<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut prior = None;
    for value in values {
        if !valid_text(value, MAX_STRING_BYTES) || prior.is_some_and(|item| item >= value) {
            return false;
        }
        prior = Some(value);
    }
    true
}
fn valid_correlation(value: &Correlation) -> bool {
    if value.durable_position == Some(0) {
        return false;
    }
    if value.trace_id.as_deref().is_some_and(|v| !hex_id(v, 32))
        || value.span_id.as_deref().is_some_and(|v| !hex_id(v, 16))
        || value
            .parent_span_id
            .as_deref()
            .is_some_and(|v| !hex_id(v, 16))
    {
        return false;
    }
    [
        &value.session_id,
        &value.turn_id,
        &value.execution_id,
        &value.model_request_id,
        &value.tool_invocation_id,
    ]
    .into_iter()
    .flatten()
    .all(|v| valid_text(v, MAX_ID_BYTES))
}
fn hex_id(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && value.bytes().any(|b| b != b'0')
}
fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.trim() == value && value.len() <= max
}
fn canonical_time(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}
const fn error(code: AgentSignalErrorCode) -> AgentSignalError {
    AgentSignalError::new(code)
}
