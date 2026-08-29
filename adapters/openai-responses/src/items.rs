//! Typed portable output-item and content discriminators.

use crate::ItemStatus;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// A lossless protocol object outside the portable item profile.
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolExtension {
    discriminator: String,
    object: Map<String, Value>,
}

impl ProtocolExtension {
    /// Creates an extension after validating its discriminator.
    pub fn new(
        discriminator: impl Into<String>,
        mut object: Map<String, Value>,
    ) -> Result<Self, &'static str> {
        let discriminator = discriminator.into();
        if discriminator.is_empty() {
            return Err("Responses extension discriminator must not be empty");
        }
        object.insert("type".into(), Value::String(discriminator.clone()));
        Ok(Self {
            discriminator,
            object,
        })
    }

    /// Returns the original protocol discriminator.
    pub fn discriminator(&self) -> &str {
        &self.discriminator
    }

    /// Returns the lossless original object including `type`.
    pub fn object(&self) -> &Map<String, Value> {
        &self.object
    }
}

/// Portable response output-item union with lossless extensions.
#[derive(Clone, Debug, PartialEq)]
pub enum ResponseOutputItem {
    /// Assistant message output.
    Message(ResponseMessage),
    /// Client function call output.
    FunctionCall(ResponseFunctionCall),
    /// Model reasoning output.
    Reasoning(ResponseReasoning),
    /// Hosted, future, or Provider-specific output item.
    Extension(ProtocolExtension),
}

impl Serialize for ResponseOutputItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Message(value) => value.serialize(serializer),
            Self::FunctionCall(value) => value.serialize(serializer),
            Self::Reasoning(value) => value.serialize(serializer),
            Self::Extension(value) => value.object.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponseOutputItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("Responses output item must be an object"))?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("Responses output item requires type"))?;
        match kind {
            "message" => serde_json::from_value(value)
                .map(Self::Message)
                .map_err(D::Error::custom),
            "function_call" => serde_json::from_value(value)
                .map(Self::FunctionCall)
                .map_err(D::Error::custom),
            "reasoning" => serde_json::from_value(value)
                .map(Self::Reasoning)
                .map_err(D::Error::custom),
            _ => ProtocolExtension::new(kind, object.clone())
                .map(Self::Extension)
                .map_err(D::Error::custom),
        }
    }
}

/// Assistant message output item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponseMessage {
    /// Unique output item identifier.
    pub id: String,
    /// Ordered portable message content.
    pub content: Vec<OutputContent>,
    /// Fixed assistant role.
    pub role: AssistantRole,
    /// Item lifecycle status.
    pub status: ItemStatus,
    /// Optional official output phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
    #[serde(rename = "type")]
    kind: MessageType,
}

/// Fixed assistant response role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantRole {
    /// Model assistant output.
    Assistant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MessageType {
    Message,
}

/// Client function call output item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionCall {
    /// Optional output item identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Stable correlation identifier.
    pub call_id: String,
    /// Function name.
    pub name: String,
    /// Complete JSON argument text.
    pub arguments: String,
    /// Optional item lifecycle status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ItemStatus>,
    /// Optional function namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Optional official caller descriptor retained losslessly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
    #[serde(rename = "type")]
    kind: FunctionCallType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FunctionCallType {
    FunctionCall,
}

/// Reasoning output item retained as protocol data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoning {
    /// Unique output item identifier.
    pub id: String,
    /// Ordered reasoning summary parts.
    pub summary: Vec<ReasoningPart>,
    /// Optional ordered reasoning text parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ReasoningPart>>,
    /// Optional opaque encrypted protocol state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    /// Optional item lifecycle status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ItemStatus>,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
    #[serde(rename = "type")]
    kind: ReasoningType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReasoningType {
    Reasoning,
}

/// Summary or visible reasoning text part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReasoningPart {
    /// Part discriminator, normally `summary_text` or `reasoning_text`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Part text.
    pub text: String,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

/// Portable assistant message content union.
#[derive(Clone, Debug, PartialEq)]
pub enum OutputContent {
    /// Model text output.
    OutputText(OutputText),
    /// Model refusal output.
    Refusal(OutputRefusal),
    /// Future or Provider-specific content.
    Extension(ProtocolExtension),
}

impl Serialize for OutputContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::OutputText(value) => value.serialize(serializer),
            Self::Refusal(value) => value.serialize(serializer),
            Self::Extension(value) => value.object.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for OutputContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("Responses content must be an object"))?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("Responses content requires type"))?;
        match kind {
            "output_text" => serde_json::from_value(value)
                .map(Self::OutputText)
                .map_err(D::Error::custom),
            "refusal" => serde_json::from_value(value)
                .map(Self::Refusal)
                .map_err(D::Error::custom),
            _ => ProtocolExtension::new(kind, object.clone())
                .map(Self::Extension)
                .map_err(D::Error::custom),
        }
    }
}

/// Text output content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputText {
    /// Complete text.
    pub text: String,
    /// Lossless official annotation objects.
    #[serde(default)]
    pub annotations: Vec<Value>,
    /// Optional lossless token log probabilities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logprobs: Vec<Value>,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
    #[serde(rename = "type")]
    kind: OutputTextType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OutputTextType {
    OutputText,
}

/// Refusal output content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputRefusal {
    /// Complete refusal text.
    pub refusal: String,
    /// Non-colliding future fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
    #[serde(rename = "type")]
    kind: RefusalType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RefusalType {
    Refusal,
}
