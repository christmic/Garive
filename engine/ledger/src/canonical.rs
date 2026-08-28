use std::fmt::Write;

use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Canonical JSON payload and the SHA-256 digest that binds its stored bytes.
pub struct CanonicalPayload {
    json: String,
    sha256: String,
}

impl CanonicalPayload {
    /// Canonicalizes a JSON value and computes its lowercase SHA-256 digest.
    pub fn from_value(value: &Value) -> Result<Self, CanonicalPayloadError> {
        let mut json = String::new();
        encode(value, &mut json)?;
        let sha256 = digest(json.as_bytes());
        Ok(Self { json, sha256 })
    }

    /// Validates persisted JSON and digest bytes before reconstructing a payload.
    pub fn from_canonical_parts(
        json: String,
        sha256: String,
    ) -> Result<Self, CanonicalPayloadError> {
        let value: Value =
            serde_json::from_str(&json).map_err(|_| CanonicalPayloadError::InvalidJson)?;
        let canonical = Self::from_value(&value)?;
        if canonical.json != json {
            return Err(CanonicalPayloadError::NonCanonical);
        }
        if canonical.sha256 != sha256 {
            return Err(CanonicalPayloadError::DigestMismatch);
        }
        Ok(canonical)
    }

    /// Recomputes the digest and fails when the payload bytes were corrupted.
    pub fn verify(&self) -> Result<(), CanonicalPayloadError> {
        if digest(self.json.as_bytes()) == self.sha256 {
            Ok(())
        } else {
            Err(CanonicalPayloadError::DigestMismatch)
        }
    }

    /// Returns the canonical UTF-8 JSON representation.
    pub fn as_json(&self) -> &str {
        &self.json
    }

    /// Returns the lowercase hexadecimal SHA-256 digest.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[doc(hidden)]
    pub fn with_digest_for_corruption_test(&self, sha256: impl Into<String>) -> Self {
        Self {
            json: self.json.clone(),
            sha256: sha256.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Failure while canonicalizing or validating a persisted payload.
pub enum CanonicalPayloadError {
    /// Stored bytes are not valid JSON.
    InvalidJson,
    /// The JSON number is outside the admitted integer-only surface.
    UnsupportedNumber,
    /// Stored JSON is valid but not in the required canonical representation.
    NonCanonical,
    /// Stored or in-memory payload bytes do not match the bound digest.
    DigestMismatch,
}

fn encode(value: &Value, output: &mut String) -> Result<(), CanonicalPayloadError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) if value.is_i64() || value.is_u64() => {
            output.push_str(&value.to_string())
        }
        Value::Number(_) => return Err(CanonicalPayloadError::UnsupportedNumber),
        Value::String(value) => encode_string(value, output),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                encode(value, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                encode_string(key, output);
                output.push(':');
                encode(value, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn encode_string(value: &str, output: &mut String) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value <= '\u{1f}' => {
                write!(output, "\\u{:04x}", u32::from(value))
                    .expect("writing to String cannot fail");
            }
            value => output.push(value),
        }
    }
    output.push('"');
}

fn digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
