use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use serde::{de::MapAccess, de::SeqAccess, Deserialize, Deserializer};
use serde_json::{Map, Number, Value};

/// Maximum accepted UTF-8 configuration document size.
pub const MAX_DESKTOP_CONFIG_BYTES: usize = 65_536;

/// Stable secret-free Desktop configuration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopConfigurationError {
    /// No configuration document exists at the explicit location.
    NotPresent,
    /// The explicit file could not be read safely.
    ReadFailure,
    /// The document exceeded the configured byte bound.
    TooLarge,
    /// JSON, fields or duplicate-member rules were invalid.
    InvalidDocument,
    /// The schema version is not supported.
    UnsupportedVersion,
    /// A configured relative storage path was unsafe.
    InvalidPath,
    /// One required value or bound was invalid.
    InvalidValue,
    /// The backend secret resolver did not produce a credential.
    SecretUnavailable,
    /// No exact installed profile owns the configured profile identity.
    UnknownProfile,
    /// Explicit values could not construct the Runtime composition.
    ConstructionFailure,
}

impl DesktopConfigurationError {
    /// Returns the stable diagnostic code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotPresent => "not_present",
            Self::ReadFailure => "read_failure",
            Self::TooLarge => "too_large",
            Self::InvalidDocument => "invalid_document",
            Self::UnsupportedVersion => "unsupported_version",
            Self::InvalidPath => "invalid_path",
            Self::InvalidValue => "invalid_value",
            Self::SecretUnavailable => "secret_unavailable",
            Self::UnknownProfile => "unknown_profile",
            Self::ConstructionFailure => "construction_failure",
        }
    }
}

/// Validated non-secret snapshot read by Desktop startup.
pub struct DesktopSystemConfiguration {
    pub(crate) database_path: PathBuf,
    pub(crate) installed_agent: InstalledAgentDocument,
    pub(crate) host: HostDocument,
    pub(crate) execution: ExecutionDocument,
    pub(crate) http: HttpDocument,
    pub(crate) dispatch_capacity: usize,
    pub(crate) execution_lease_duration_ms: u64,
}

impl fmt::Debug for DesktopSystemConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopSystemConfiguration")
            .field("database_path", &self.database_path)
            .field("profile_id", &self.execution.profile_id)
            .field("credential_ref", &"<redacted-reference>")
            .field("model_target_id", &self.execution.model_target_id)
            .field("dispatch_capacity", &self.dispatch_capacity)
            .finish()
    }
}

impl DesktopSystemConfiguration {
    /// Parses one exact bounded document relative to an explicit app config directory.
    pub fn parse(
        bytes: &[u8],
        app_config_directory: &Path,
    ) -> Result<Self, DesktopConfigurationError> {
        if bytes.len() > MAX_DESKTOP_CONFIG_BYTES {
            return Err(DesktopConfigurationError::TooLarge);
        }
        let value = unique_json(bytes)?;
        let raw: RawDocument = serde_json::from_value(value)
            .map_err(|_| DesktopConfigurationError::InvalidDocument)?;
        if raw.schema_version != 1 {
            return Err(DesktopConfigurationError::UnsupportedVersion);
        }
        let database_file = Path::new(&raw.database_file);
        if database_file.components().count() != 1
            || !matches!(
                database_file.components().next(),
                Some(Component::Normal(_))
            )
        {
            return Err(DesktopConfigurationError::InvalidPath);
        }
        validate(&raw)?;
        Ok(Self {
            database_path: app_config_directory.join(database_file),
            installed_agent: raw.installed_agent,
            host: raw.host,
            execution: raw.execution,
            http: raw.http,
            dispatch_capacity: raw.dispatch_capacity,
            execution_lease_duration_ms: raw.execution_lease_duration_ms,
        })
    }

    /// Returns the resolved database path without exposing connection configuration.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Returns the opaque installed profile identity.
    pub fn profile_id(&self) -> &str {
        &self.execution.profile_id
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    schema_version: u32,
    database_file: String,
    installed_agent: InstalledAgentDocument,
    host: HostDocument,
    execution: ExecutionDocument,
    http: HttpDocument,
    dispatch_capacity: usize,
    execution_lease_duration_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstalledAgentDocument {
    pub(crate) definition_id: String,
    pub(crate) definition_revision: String,
    pub(crate) snapshot_digest: String,
    pub(crate) agent_instance_namespace: String,
    pub(crate) max_iterations: u64,
    pub(crate) max_input_tokens: Option<u64>,
    pub(crate) max_output_tokens: Option<u64>,
    pub(crate) deadline_budget_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostDocument {
    pub(crate) max_command_bytes: usize,
    pub(crate) event_batch_size: u64,
    pub(crate) event_poll_interval_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionDocument {
    pub(crate) profile_id: String,
    pub(crate) credential_ref: String,
    pub(crate) endpoint: Option<String>,
    pub(crate) model_target_id: String,
    pub(crate) model_id: String,
    pub(crate) deployment_id: String,
    pub(crate) recovery_policy_revision: String,
    pub(crate) max_output_tokens: Option<u64>,
    pub(crate) max_context_items: usize,
    pub(crate) max_context_utf8_bytes: usize,
    pub(crate) max_model_attempts: u32,
    pub(crate) max_context_rebuilds: u32,
    pub(crate) output_limit_action: OutputLimitDocument,
    pub(crate) output_limit_max_retries: Option<u32>,
    pub(crate) transport_action: TerminalActionDocument,
    pub(crate) unavailable_action: TerminalActionDocument,
    pub(crate) missing_usage_policy: MissingUsageDocument,
    pub(crate) missing_usage_estimate_input_tokens: Option<u64>,
    pub(crate) missing_usage_estimate_output_tokens: Option<u64>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OutputLimitDocument {
    CompletePartial,
    Retry,
    Suspend,
    Stop,
    Fail,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TerminalActionDocument {
    Suspend,
    Stop,
    Fail,
    AlternateThenSuspend,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MissingUsageDocument {
    Estimate,
    Stop,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HttpDocument {
    pub(crate) connect_timeout_ms: u64,
    pub(crate) request_timeout_ms: u64,
    pub(crate) max_response_bytes: usize,
}

fn validate(raw: &RawDocument) -> Result<(), DesktopConfigurationError> {
    let texts = [
        raw.installed_agent.definition_id.as_str(),
        raw.installed_agent.definition_revision.as_str(),
        raw.installed_agent.agent_instance_namespace.as_str(),
        raw.execution.profile_id.as_str(),
        raw.execution.credential_ref.as_str(),
        raw.execution.model_target_id.as_str(),
        raw.execution.model_id.as_str(),
        raw.execution.deployment_id.as_str(),
        raw.execution.recovery_policy_revision.as_str(),
    ];
    if texts
        .iter()
        .any(|value| value.is_empty() || value.len() > 256)
        || raw.installed_agent.snapshot_digest.len() != 64
        || !raw
            .installed_agent
            .snapshot_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || raw
            .execution
            .endpoint
            .as_ref()
            .is_some_and(String::is_empty)
        || raw.installed_agent.max_iterations == 0
        || optional_zero(raw.installed_agent.max_input_tokens)
        || optional_zero(raw.installed_agent.max_output_tokens)
        || optional_zero(raw.installed_agent.deadline_budget_ms)
        || optional_zero(raw.execution.max_output_tokens)
        || raw.host.max_command_bytes == 0
        || raw.host.event_batch_size == 0
        || raw.host.event_poll_interval_ms == 0
        || raw.execution.max_context_items == 0
        || raw.execution.max_context_utf8_bytes == 0
        || raw.execution.max_model_attempts == 0
        || raw.http.connect_timeout_ms == 0
        || raw.http.request_timeout_ms == 0
        || raw.http.max_response_bytes == 0
        || raw.dispatch_capacity == 0
        || raw.execution_lease_duration_ms == 0
        || !valid_output_limit(&raw.execution)
        || !valid_missing_usage(&raw.execution)
    {
        return Err(DesktopConfigurationError::InvalidValue);
    }
    Ok(())
}

fn valid_output_limit(execution: &ExecutionDocument) -> bool {
    match execution.output_limit_action {
        OutputLimitDocument::Retry => execution.output_limit_max_retries.is_some_and(|v| v > 0),
        _ => execution.output_limit_max_retries.is_none(),
    }
}

fn valid_missing_usage(execution: &ExecutionDocument) -> bool {
    match execution.missing_usage_policy {
        MissingUsageDocument::Estimate => {
            execution
                .missing_usage_estimate_input_tokens
                .is_some_and(|value| value > 0)
                && execution
                    .missing_usage_estimate_output_tokens
                    .is_some_and(|value| value > 0)
        }
        MissingUsageDocument::Stop => {
            execution.missing_usage_estimate_input_tokens.is_none()
                && execution.missing_usage_estimate_output_tokens.is_none()
        }
    }
}

fn optional_zero(value: Option<u64>) -> bool {
    value == Some(0)
}

fn unique_json(bytes: &[u8]) -> Result<Value, DesktopConfigurationError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueValue::deserialize(&mut deserializer)
        .map_err(|_| DesktopConfigurationError::InvalidDocument)?
        .0;
    deserializer
        .end()
        .map_err(|_| DesktopConfigurationError::InvalidDocument)?;
    Ok(value)
}

struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;
impl<'de> serde::de::Visitor<'de> for UniqueVisitor {
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
