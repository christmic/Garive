use serde::{Deserialize, Serialize};

/// Wire schema version stamped on every [`ManagementCommitBody`].
///
/// The [`LiveHost`] layer rejects any body whose `schema_version` differs
/// from this constant, forcing an explicit client rollover rather than
/// silent drift.
pub const MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION: u32 = 1;

/// Wire schema version stamped on every [`ManagementConfigReceipt`].
pub const MANAGEMENT_RECEIPT_SCHEMA_VERSION: u32 = 1;

/// Maximum number of UTF-8 bytes permitted in `endpoint_override`.
pub const MAX_ENDPOINT_BYTES: usize = 256;

/// Maximum number of UTF-8 bytes permitted in `api_key`.
pub const MAX_API_KEY_BYTES: usize = 512;

/// Maximum number of UTF-8 bytes permitted in `runtime_id`.
pub const MAX_RUNTIME_ID_BYTES: usize = 128;

/// Maximum number of UTF-8 bytes permitted in any `*_id` field
/// (`profile_id`, `definition_id`, `model_target_id`, `model_id`,
/// `deployment_id`).
pub const MAX_ID_BYTES: usize = 128;

/// Wire body for `POST /v1/management/setup`.
///
/// Every successful commit replaces the singleton row in
/// `runtime_management_config`, bumping `configuration_revision` and
/// recomputing `configuration_digest`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManagementCommitBody {
    /// Wire schema version; MUST equal [`MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Built-in Provider profile id (e.g. `openai.responses.v1`).
    pub profile_id: String,
    /// Optional Provider endpoint override; bounded by [`MAX_ENDPOINT_BYTES`].
    pub endpoint_override: Option<String>,
    /// Logical model-target id surfaced in receipts and durable facts.
    pub model_target_id: String,
    /// Concrete upstream model id.
    pub model_id: String,
    /// Logical deployment id used by the Provider profile.
    pub deployment_id: String,
    /// Built-in agent definition id (e.g. `desktop.agent.v3`).
    pub definition_id: String,
    /// Provider API key; plaintext per current Runtime management-port contract.
    pub api_key: String,
    /// Stable Runtime identifier the commit binds to.
    pub runtime_id: String,
}

/// Internal Runtime view of the committed configuration.
///
/// The `api_key` field is intentionally absent — read paths MUST NOT echo
/// credentials back over the wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagementConfigState {
    /// Built-in Provider profile id.
    pub profile_id: String,
    /// Optional Provider endpoint override.
    pub endpoint_override: Option<String>,
    /// Logical model-target id.
    pub model_target_id: String,
    /// Concrete upstream model id.
    pub model_id: String,
    /// Logical deployment id used by the Provider profile.
    pub deployment_id: String,
    /// Built-in agent definition id.
    pub definition_id: String,
    /// Stable Runtime identifier the commit binds to.
    pub runtime_id: String,
    /// Monotonic version advanced on every successful commit.
    pub configuration_revision: u64,
    /// SHA-256 hex digest of the canonical commit envelope.
    pub configuration_digest: String,
    /// RFC 3339 commit timestamp recorded by the host clock.
    pub committed_at: String,
}

/// Internal Runtime view of the committed configuration INCLUDING the
/// plaintext `api_key`. Reserved for trusted in-process callers (the
/// headless binary, integration tests). It is **never** serialized to the
/// H1 wire — read paths use [`ManagementConfigState`] instead.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagementConfigStateWithCredential {
    /// Field-for-field mirror of [`ManagementConfigState`].
    pub state: ManagementConfigState,
    /// Plaintext Provider API key.
    pub api_key: String,
}

/// Wire receipt returned from `POST /v1/management/setup`.
///
/// `restart_required` is always `true`: the Runtime does not hot-swap
/// committed configuration (matching the existing Setup receipt contract).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementConfigReceipt {
    /// Wire schema version of this receipt.
    pub schema_version: u32,
    /// Monotonic version assigned by the committed row.
    pub configuration_revision: u64,
    /// SHA-256 hex digest of the canonical commit envelope.
    pub configuration_digest: String,
    /// Always `true`; commits never hot-swap.
    pub restart_required: bool,
    /// SHA-256 hex digest of this receipt (binds revision + digest + restart).
    pub receipt_digest: String,
}

/// Domain failure from a management configuration operation.
///
/// The `wire_code()` mapping is part of the stable HTTP contract: clients
/// branch on the code string, not the enum variant.
#[derive(Debug, Eq, PartialEq)]
pub enum ManagementConfigError {
    /// `profile_id` is not in the built-in profile registry.
    ProfileUnknown,
    /// `definition_id` is not in the built-in agent registry.
    DefinitionUnknown,
    /// `endpoint_override` failed length or character validation.
    EndpointInvalid,
    /// `api_key` was empty after trimming or exceeded the byte cap.
    ApiKeyInvalid,
    /// `runtime_id` failed length or character validation.
    RuntimeIdInvalid,
    /// Any of the `*_id` fields failed length validation.
    IdentifierInvalid,
    /// `schema_version` did not equal the current wire version.
    SchemaVersionUnsupported,
    /// SQLite returned an operational or constraint error.
    StorageFailed,
    /// `GET /v1/management/setup` ran before any successful commit.
    NotConfigured,
}

impl ManagementConfigError {
    /// Stable wire code emitted in the HTTP error body.
    pub const fn wire_code(&self) -> &'static str {
        match self {
            Self::ProfileUnknown => "management_profile_unknown",
            Self::DefinitionUnknown => "management_definition_unknown",
            Self::EndpointInvalid => "management_endpoint_invalid",
            Self::ApiKeyInvalid => "management_api_key_invalid",
            Self::RuntimeIdInvalid => "management_runtime_id_invalid",
            Self::IdentifierInvalid => "management_identifier_invalid",
            Self::SchemaVersionUnsupported => "management_schema_version_unsupported",
            Self::StorageFailed => "management_storage_failed",
            Self::NotConfigured => "management_not_configured",
        }
    }
}

impl std::fmt::Display for ManagementConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.wire_code())
    }
}

impl std::error::Error for ManagementConfigError {}
