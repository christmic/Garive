use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::digest::{
    configuration_digest, receipt_digest, ConfigurationDigestEnvelope, ReceiptDigestEnvelope,
};
use super::types::{
    ManagementCommitBody, ManagementConfigError, ManagementConfigReceipt, ManagementConfigState,
    ManagementConfigStateWithCredential, MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
    MANAGEMENT_RECEIPT_SCHEMA_VERSION, MAX_API_KEY_BYTES, MAX_ENDPOINT_BYTES, MAX_ID_BYTES,
    MAX_RUNTIME_ID_BYTES,
};
use ManagementConfigError::{
    ApiKeyInvalid, EndpointInvalid, IdentifierInvalid, RuntimeIdInvalid, SchemaVersionUnsupported,
    StorageFailed,
};

const SELECT_COLUMNS: &str = "profile_id, endpoint_override, model_target_id, model_id, \
    deployment_id, definition_id, runtime_id, configuration_revision, \
    configuration_digest, committed_at";

/// All singleton-row columns INCLUDING the plaintext `api_key`. Reserved for
/// trusted internal callers via [`ManagementConfigStore::read_with_credential`].
const SELECT_COLUMNS_WITH_CREDENTIAL: &str =
    "profile_id, endpoint_override, model_target_id, model_id, deployment_id, \
    definition_id, api_key, runtime_id, configuration_revision, configuration_digest, committed_at";

/// DAO over the singleton `runtime_management_config` row introduced in
/// SQLite schema v9.
///
/// Constructed via [`SqliteLedger::management_config_store`]; never owned
/// independently so the connection lifetime stays tied to the Ledger.
pub struct ManagementConfigStore<'a> {
    connection: &'a mut Connection,
}

impl<'a> ManagementConfigStore<'a> {
    /// Wraps the given SQLite connection. Validates the connection only at
    /// use time — opening the Ledger is the upstream invariant.
    #[doc(hidden)]
    #[allow(dead_code)] // exercised by tests + commit-3 handler
    pub fn new(connection: &'a mut Connection) -> Self {
        Self { connection }
    }

    /// Reads the committed singleton row, returning `Ok(None)` when the
    /// table is empty (no commit has ever succeeded).
    pub fn read(&self) -> Result<Option<ManagementConfigState>, ManagementConfigError> {
        let query =
            format!("SELECT {SELECT_COLUMNS} FROM runtime_management_config WHERE config_id = 1");
        self.connection
            .query_row(&query, [], row_to_state)
            .optional()
            .map_err(|_| StorageFailed)
    }

    /// Reads the committed singleton row INCLUDING the plaintext `api_key`.
    ///
    /// Reserved for trusted in-process callers (the headless binary, the
    /// management validator tests, integration tests that need to drive
    /// `RuntimeModelHttpTransport`). The returned wrapper is **never**
    /// serialized to the H1 wire — [`Self::read`] is the public surface for
    /// any caller that might.
    pub fn read_with_credential(
        &self,
    ) -> Result<Option<ManagementConfigStateWithCredential>, ManagementConfigError> {
        let query = format!(
            "SELECT {SELECT_COLUMNS_WITH_CREDENTIAL} FROM runtime_management_config WHERE config_id = 1"
        );
        self.connection
            .query_row(&query, [], row_to_state_with_credential)
            .optional()
            .map_err(|_| StorageFailed)
    }

    /// Replaces the singleton row with the validated commit body, bumping
    /// `configuration_revision` and writing the recomputed digest inside one
    /// `IMMEDIATE` transaction.
    ///
    /// Validation runs before the transaction starts; the SQLite store
    /// returns only `StorageFailed` for any persistence-layer fault.
    pub fn commit(
        &mut self,
        body: &ManagementCommitBody,
        committed_at: &str,
    ) -> Result<ManagementConfigReceipt, ManagementConfigError> {
        validate_body(body)?;
        validate_timestamp(committed_at)?;

        let envelope = ConfigurationDigestEnvelope {
            api_key: body.api_key.as_str(),
            definition_id: body.definition_id.as_str(),
            deployment_id: body.deployment_id.as_str(),
            endpoint_override: body.endpoint_override.as_deref(),
            model_id: body.model_id.as_str(),
            model_target_id: body.model_target_id.as_str(),
            profile_id: body.profile_id.as_str(),
            runtime_id: body.runtime_id.as_str(),
        };
        let digest = configuration_digest(&envelope);

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| StorageFailed)?;
        let previous_revision: i64 = transaction
            .query_row(
                "SELECT configuration_revision FROM runtime_management_config WHERE config_id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| StorageFailed)?
            .unwrap_or(0);
        let new_revision = previous_revision.checked_add(1).ok_or(StorageFailed)?;

        transaction
            .execute(
                "INSERT OR REPLACE INTO runtime_management_config(\
                    config_id, profile_id, endpoint_override, model_target_id, model_id, \
                    deployment_id, definition_id, api_key, runtime_id, \
                    configuration_revision, configuration_digest, committed_at\
                 ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    body.profile_id,
                    body.endpoint_override,
                    body.model_target_id,
                    body.model_id,
                    body.deployment_id,
                    body.definition_id,
                    body.api_key,
                    body.runtime_id,
                    new_revision,
                    digest,
                    committed_at,
                ],
            )
            .map_err(|_| StorageFailed)?;
        transaction.commit().map_err(|_| StorageFailed)?;

        let new_revision_u64 = u64::try_from(new_revision).map_err(|_| StorageFailed)?;
        let receipt_digest_value = receipt_digest(&ReceiptDigestEnvelope {
            configuration_digest: digest.as_str(),
            configuration_revision: new_revision_u64,
            restart_required: true,
        });
        let receipt = ManagementConfigReceipt {
            schema_version: MANAGEMENT_RECEIPT_SCHEMA_VERSION,
            configuration_revision: new_revision_u64,
            configuration_digest: digest,
            restart_required: true,
            receipt_digest: receipt_digest_value,
        };
        Ok(receipt)
    }

    /// Deletes the singleton row, leaving the table empty.
    pub fn clear(&self) -> Result<(), ManagementConfigError> {
        self.connection
            .execute(
                "DELETE FROM runtime_management_config WHERE config_id = 1",
                [],
            )
            .map_err(|_| StorageFailed)?;
        Ok(())
    }
}

fn row_to_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<ManagementConfigState> {
    let revision: i64 = row.get(7)?;
    let revision = u64::try_from(revision).map_err(|_| {
        rusqlite::Error::InvalidColumnType(
            7,
            "configuration_revision".into(),
            rusqlite::types::Type::Integer,
        )
    })?;
    Ok(ManagementConfigState {
        profile_id: row.get(0)?,
        endpoint_override: row.get(1)?,
        model_target_id: row.get(2)?,
        model_id: row.get(3)?,
        deployment_id: row.get(4)?,
        definition_id: row.get(5)?,
        runtime_id: row.get(6)?,
        configuration_revision: revision,
        configuration_digest: row.get(8)?,
        committed_at: row.get(9)?,
    })
}

fn row_to_state_with_credential(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagementConfigStateWithCredential> {
    let revision: i64 = row.get(8)?;
    let revision = u64::try_from(revision).map_err(|_| {
        rusqlite::Error::InvalidColumnType(
            8,
            "configuration_revision".into(),
            rusqlite::types::Type::Integer,
        )
    })?;
    Ok(ManagementConfigStateWithCredential {
        state: ManagementConfigState {
            profile_id: row.get(0)?,
            endpoint_override: row.get(1)?,
            model_target_id: row.get(2)?,
            model_id: row.get(3)?,
            deployment_id: row.get(4)?,
            definition_id: row.get(5)?,
            runtime_id: row.get(7)?,
            configuration_revision: revision,
            configuration_digest: row.get(9)?,
            committed_at: row.get(10)?,
        },
        api_key: row.get(6)?,
    })
}

fn validate_body(body: &ManagementCommitBody) -> Result<(), ManagementConfigError> {
    if body.schema_version != MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION {
        return Err(SchemaVersionUnsupported);
    }
    validate_id(&body.profile_id, MAX_ID_BYTES)?;
    validate_id(&body.definition_id, MAX_ID_BYTES)?;
    validate_id(&body.model_target_id, MAX_ID_BYTES)?;
    validate_id(&body.model_id, MAX_ID_BYTES)?;
    validate_id(&body.deployment_id, MAX_ID_BYTES)?;
    validate_runtime_id(&body.runtime_id)?;
    validate_api_key(&body.api_key)?;
    if let Some(endpoint) = body.endpoint_override.as_deref() {
        validate_endpoint(endpoint)?;
    }
    Ok(())
}

fn validate_id(value: &str, max_bytes: usize) -> Result<(), ManagementConfigError> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(IdentifierInvalid);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(IdentifierInvalid);
    }
    Ok(())
}

fn validate_runtime_id(value: &str) -> Result<(), ManagementConfigError> {
    if value.is_empty() || value.len() > MAX_RUNTIME_ID_BYTES {
        return Err(RuntimeIdInvalid);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RuntimeIdInvalid);
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<(), ManagementConfigError> {
    if value.is_empty() || value.len() > MAX_ENDPOINT_BYTES {
        return Err(EndpointInvalid);
    }
    if value
        .bytes()
        .any(|byte| byte == b' ' || byte == b'\n' || byte == b'\r' || byte == b'\t')
    {
        return Err(EndpointInvalid);
    }
    Ok(())
}

fn validate_api_key(value: &str) -> Result<(), ManagementConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_API_KEY_BYTES {
        return Err(ApiKeyInvalid);
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> Result<(), ManagementConfigError> {
    if value.is_empty() || value.len() > 64 {
        return Err(StorageFailed);
    }
    Ok(())
}
