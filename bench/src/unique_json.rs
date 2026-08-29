use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::{BenchError, BenchErrorCode};

pub(crate) fn unique_json(bytes: &[u8]) -> Result<Value, BenchError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueValue::deserialize(&mut deserializer)
        .map_err(|_| BenchError::new(BenchErrorCode::InvalidCaseDocument))?
        .0;
    deserializer
        .end()
        .map_err(|_| BenchError::new(BenchErrorCode::InvalidCaseDocument))?;
    Ok(value)
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
        formatter.write_str("one duplicate-free JSON value")
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
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value.to_owned())))
    }
    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = input.next_element::<UniqueValue>()? {
            values.push(value.0);
        }
        Ok(UniqueValue(Value::Array(values)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
        let mut values = Map::new();
        while let Some((key, value)) = input.next_entry::<String, UniqueValue>()? {
            if values.insert(key, value.0).is_some() {
                return Err(serde::de::Error::custom("duplicate object key"));
            }
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}
