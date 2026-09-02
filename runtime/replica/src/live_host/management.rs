use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::live_host::http::decode_body;
use crate::live_host::{LiveHost, LiveHostError};
use crate::management::{
    ManagementCommitBody, ManagementConfigError, ManagementConfigState,
    MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
};

/// Stable read view returned by `GET /v1/management/setup`.
///
/// Deliberately omits the persisted `api_key`; only metadata is surfaced.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementConfigRead {
    pub schema_version: u32,
    pub profile_id: String,
    pub endpoint_override: Option<String>,
    pub model_target_id: String,
    pub model_id: String,
    pub deployment_id: String,
    pub definition_id: String,
    pub runtime_id: String,
    pub configuration_revision: u64,
    pub configuration_digest: String,
    pub committed_at: String,
}

impl ManagementConfigRead {
    fn from_state(state: ManagementConfigState) -> Self {
        Self {
            schema_version: MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
            profile_id: state.profile_id,
            endpoint_override: state.endpoint_override,
            model_target_id: state.model_target_id,
            model_id: state.model_id,
            deployment_id: state.deployment_id,
            definition_id: state.definition_id,
            runtime_id: state.runtime_id,
            configuration_revision: state.configuration_revision,
            configuration_digest: state.configuration_digest,
            committed_at: state.committed_at,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementHealthResponse {
    pub schema_version: u32,
    pub configured: bool,
    pub configuration_revision: Option<u64>,
}

pub async fn read_setup(State(host): State<LiveHost>) -> Response {
    let result = read_setup_internal(&host);
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => management_error_response(error),
    }
}

pub async fn commit_setup(State(host): State<LiveHost>, body: Body) -> Response {
    let result = async {
        let body: ManagementCommitBody = match decode_body(&host, body).await {
            Ok(value) => value,
            Err(_) => return Err(ManagementConfigError::StorageFailed),
        };
        host.management_validator().validate(&body)?;
        let recorded_at = host.recorded_at_string();
        let mut ledger = host.open_management_ledger().map_err(map_open)?;
        ledger.management_config_store().commit(&body, &recorded_at)
    }
    .await;
    match result {
        Ok(receipt) => (StatusCode::OK, Json(receipt)).into_response(),
        Err(error) => management_error_response(error),
    }
}

pub async fn clear_setup(State(host): State<LiveHost>) -> Response {
    let result = clear_setup_internal(&host);
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => management_error_response(error),
    }
}

pub async fn health(State(host): State<LiveHost>) -> Response {
    let result = health_internal(&host);
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => management_error_response(error),
    }
}

fn read_setup_internal(host: &LiveHost) -> Result<ManagementConfigRead, ManagementConfigError> {
    let mut ledger = host.open_management_ledger().map_err(map_open)?;
    let state = ledger
        .management_config_store()
        .read()?
        .ok_or(ManagementConfigError::NotConfigured)?;
    Ok(ManagementConfigRead::from_state(state))
}

fn clear_setup_internal(host: &LiveHost) -> Result<(), ManagementConfigError> {
    let mut ledger = host.open_management_ledger().map_err(map_open)?;
    ledger.management_config_store().clear()
}

fn health_internal(host: &LiveHost) -> Result<ManagementHealthResponse, ManagementConfigError> {
    let mut ledger = host.open_management_ledger().map_err(map_open)?;
    let state = ledger.management_config_store().read()?;
    Ok(ManagementHealthResponse {
        schema_version: MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
        configured: state.is_some(),
        configuration_revision: state.map(|value| value.configuration_revision),
    })
}

fn management_error_response(error: ManagementConfigError) -> Response {
    let status = match error {
        ManagementConfigError::ProfileUnknown
        | ManagementConfigError::DefinitionUnknown
        | ManagementConfigError::EndpointInvalid
        | ManagementConfigError::ApiKeyInvalid
        | ManagementConfigError::RuntimeIdInvalid
        | ManagementConfigError::IdentifierInvalid
        | ManagementConfigError::SchemaVersionUnsupported => StatusCode::BAD_REQUEST,
        ManagementConfigError::StorageFailed => StatusCode::INTERNAL_SERVER_ERROR,
        ManagementConfigError::NotConfigured => StatusCode::NOT_FOUND,
    };
    (
        status,
        Json(serde_json::json!({
            "code": error.wire_code(),
            "message": error.wire_code(),
        })),
    )
        .into_response()
}

fn map_open(error: LiveHostError) -> ManagementConfigError {
    match error {
        LiveHostError::DurabilityUnavailable | LiveHostError::CorruptState => {
            ManagementConfigError::StorageFailed
        }
        _ => ManagementConfigError::StorageFailed,
    }
}
