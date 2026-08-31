//! Bounded Native Messaging stdio framing.

use std::{
    fmt,
    io::{Read, Write},
};

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::AttachedLimits;

/// Reads one complete native-endian length-prefixed JSON object.
pub fn read_frame(
    reader: &mut impl Read,
    limits: AttachedLimits,
) -> Result<Value, AttachedFrameError> {
    let mut prefix = [0_u8; 4];
    reader
        .read_exact(&mut prefix)
        .map_err(|_| AttachedFrameError::Truncated)?;
    let length = u32::from_ne_bytes(prefix) as usize;
    if length == 0 || length > limits.max_frame_bytes() {
        return Err(AttachedFrameError::BoundExceeded);
    }
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| AttachedFrameError::Truncated)?;
    let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
    let value = UniqueValue::deserialize(&mut deserializer)
        .map_err(|_| AttachedFrameError::InvalidJson)?
        .0;
    deserializer
        .end()
        .map_err(|_| AttachedFrameError::InvalidJson)?;
    if !value.is_object() {
        return Err(AttachedFrameError::InvalidMessage);
    }
    Ok(value)
}

/// Writes one complete bounded JSON object with a native-endian length prefix.
pub fn write_frame(
    writer: &mut impl Write,
    limits: AttachedLimits,
    value: &Value,
) -> Result<(), AttachedFrameError> {
    if !value.is_object() {
        return Err(AttachedFrameError::InvalidMessage);
    }
    let bytes = serde_json::to_vec(value).map_err(|_| AttachedFrameError::InvalidJson)?;
    if bytes.is_empty() || bytes.len() > limits.max_frame_bytes() {
        return Err(AttachedFrameError::BoundExceeded);
    }
    let length = u32::try_from(bytes.len()).map_err(|_| AttachedFrameError::BoundExceeded)?;
    writer
        .write_all(&length.to_ne_bytes())
        .and_then(|()| writer.write_all(&bytes))
        .and_then(|()| writer.flush())
        .map_err(|_| AttachedFrameError::WriteFailed)
}

/// Stable Native Messaging frame failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachedFrameError {
    /// The prefix or declared body ended early.
    Truncated,
    /// The frame is empty or above the explicit limit.
    BoundExceeded,
    /// The frame is not exactly one UTF-8 JSON value.
    InvalidJson,
    /// The decoded JSON value is not a protocol object.
    InvalidMessage,
    /// The complete prefix/body could not be written and flushed.
    WriteFailed,
}

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
        formatter.write_str("one duplicate-free Attached Browser JSON value")
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
            .ok_or_else(|| E::custom("non-finite Attached Browser number"))
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
                return Err(serde::de::Error::custom(
                    "duplicate Attached Browser object key",
                ));
            }
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}
