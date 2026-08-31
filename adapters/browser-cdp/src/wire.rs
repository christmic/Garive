//! Bounded CDP command and incoming message shapes.

use std::{error::Error, fmt};

use serde::{
    de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor},
    Serialize,
};
use serde_json::{Map, Number, Value};

/// One exact CDP command, optionally routed to a flat target session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CdpCommand {
    /// JavaScript-safe positive correlation identity.
    pub id: u64,
    /// Exact admitted CDP method.
    pub method: String,
    /// Method parameters.
    pub params: Value,
    /// Optional flat Target session identity.
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl CdpCommand {
    /// Validates correlation, method, parameter and session bindings.
    pub fn new(
        id: u64,
        method: impl Into<String>,
        params: Value,
        session_id: Option<String>,
    ) -> Result<Self, CdpProtocolError> {
        let value = Self {
            id,
            method: method.into(),
            params,
            session_id,
        };
        if id == 0
            || id > 9_007_199_254_740_991
            || !admitted_method(&value.method)
            || !value.params.is_object()
            || value.session_id.as_deref() == Some("")
        {
            Err(CdpProtocolError::InvalidMessage)
        } else {
            Ok(value)
        }
    }
}

/// One typed CDP protocol error response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpError {
    /// Protocol error number.
    pub code: i64,
    /// Bounded protocol message.
    pub message: String,
    /// Optional extension data retained for diagnostics.
    pub data: Option<Value>,
}

/// One bounded incoming response or event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CdpIncoming {
    /// Successful response to one exact correlation identity.
    Result {
        /// Command identity.
        id: u64,
        /// Returned result object.
        result: Value,
        /// Optional flat Target session identity.
        session_id: Option<String>,
    },
    /// Terminal protocol error for one exact correlation identity.
    Error {
        /// Command identity.
        id: u64,
        /// Typed error payload.
        error: CdpError,
        /// Optional flat Target session identity.
        session_id: Option<String>,
    },
    /// Unsolicited browser/target event.
    Event {
        /// Exact event method.
        method: String,
        /// Event parameters.
        params: Value,
        /// Optional flat Target session identity.
        session_id: Option<String>,
    },
}

/// Parses one complete UTF-8 frame under the explicit byte ceiling.
pub fn parse_incoming(
    bytes: &[u8],
    max_frame_bytes: usize,
) -> Result<CdpIncoming, CdpProtocolError> {
    if bytes.is_empty() || bytes.len() > max_frame_bytes {
        return Err(CdpProtocolError::FrameBoundExceeded);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueValue::deserialize(&mut deserializer)
        .map_err(|_| CdpProtocolError::InvalidJson)?
        .0;
    deserializer
        .end()
        .map_err(|_| CdpProtocolError::InvalidJson)?;
    let object = value.as_object().ok_or(CdpProtocolError::InvalidMessage)?;
    let session_id = optional_text(object.get("sessionId"))?;
    if let Some(id) = object.get("id") {
        let id = id
            .as_u64()
            .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
            .ok_or(CdpProtocolError::InvalidMessage)?;
        return match (object.get("result"), object.get("error")) {
            (Some(result), None) if result.is_object() => Ok(CdpIncoming::Result {
                id,
                result: result.clone(),
                session_id,
            }),
            (None, Some(error)) => Ok(CdpIncoming::Error {
                id,
                error: parse_error(error)?,
                session_id,
            }),
            _ => Err(CdpProtocolError::InvalidMessage),
        };
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(CdpProtocolError::InvalidMessage)?;
    let params = object
        .get("params")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or(CdpProtocolError::InvalidMessage)?;
    Ok(CdpIncoming::Event {
        method: method.into(),
        params,
        session_id,
    })
}

fn parse_error(value: &Value) -> Result<CdpError, CdpProtocolError> {
    let object = value.as_object().ok_or(CdpProtocolError::InvalidMessage)?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 4_096)
        .ok_or(CdpProtocolError::InvalidMessage)?;
    Ok(CdpError {
        code: object
            .get("code")
            .and_then(Value::as_i64)
            .ok_or(CdpProtocolError::InvalidMessage)?,
        message: message.into(),
        data: object.get("data").cloned(),
    })
}

fn optional_text(value: Option<&Value>) -> Result<Option<String>, CdpProtocolError> {
    value
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 4_096)
                .map(str::to_owned)
                .ok_or(CdpProtocolError::InvalidMessage)
        })
        .transpose()
}

fn admitted_method(method: &str) -> bool {
    matches!(
        method,
        "Browser.getVersion"
            | "Target.activateTarget"
            | "Target.attachToTarget"
            | "Target.closeTarget"
            | "Target.createTarget"
            | "Target.setDiscoverTargets"
            | "Accessibility.enable"
            | "Accessibility.disable"
            | "Accessibility.getFullAXTree"
            | "Page.enable"
            | "Page.navigate"
            | "Page.reload"
            | "Page.setLifecycleEventsEnabled"
            | "Page.getNavigationHistory"
            | "Page.getFrameTree"
            | "Page.navigateToHistoryEntry"
            | "Page.getLayoutMetrics"
            | "Input.dispatchKeyEvent"
            | "Input.dispatchMouseEvent"
            | "Input.insertText"
            | "DOM.focus"
            | "DOM.describeNode"
            | "DOM.getFrameOwner"
            | "DOM.scrollIntoViewIfNeeded"
            | "DOM.getBoxModel"
            | "DOM.resolveNode"
            | "Runtime.callFunctionOn"
            | "Runtime.releaseObject"
    )
}

/// Stable bounded CDP wire failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdpProtocolError {
    /// Frame is empty or above the configured byte ceiling.
    FrameBoundExceeded,
    /// Frame is not valid JSON.
    InvalidJson,
    /// JSON does not match one exact admitted command/response/event shape.
    InvalidMessage,
}

impl fmt::Display for CdpProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FrameBoundExceeded => "CDP frame bound exceeded",
            Self::InvalidJson => "invalid CDP JSON",
            Self::InvalidMessage => "invalid CDP message",
        })
    }
}

impl Error for CdpProtocolError {}

struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;

impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = UniqueValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("one duplicate-free CDP JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Number(value.into())))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Number::from_f64(value)
            .map(|number| UniqueValue(Value::Number(number)))
            .ok_or_else(|| E::custom("non-finite CDP number"))
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<UniqueValue>()? {
            values.push(value.0);
        }
        Ok(UniqueValue(Value::Array(values)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = Map::new();
        while let Some((key, value)) = map.next_entry::<String, UniqueValue>()? {
            if values.insert(key, value.0).is_some() {
                return Err(serde::de::Error::custom("duplicate CDP object key"));
            }
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}
