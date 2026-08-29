//! Neutral bounded Agent signals; Runtime owns buffering and exporters.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod catalogue;
mod signal;
mod values;

pub use catalogue::{attribute_enum_values, signal_schema, SignalSchema, SIGNAL_NAMES};
pub use signal::{AgentSignal, SignalBinding};
pub use values::{
    AgentSignalError, AgentSignalErrorCode, Attribute, AttributeValue, Correlation, Measurement,
    MeasurementUnit, MeasurementValue, RedactionClass, Severity,
};
