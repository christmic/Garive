use std::collections::BTreeMap;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{HostReadLimits, InstalledAgent, LiveHostError, SessionSummaryV1};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SessionCursorV1 {
    schema_version: u32,
    opened_at: String,
    session_id: String,
    installation_binding_digest: String,
    cursor_digest: String,
}

#[derive(Serialize)]
struct CursorDigest<'a> {
    schema_version: u32,
    opened_at: &'a str,
    session_id: &'a str,
    installation_binding_digest: &'a str,
}

pub(super) fn encode(
    session: &SessionSummaryV1,
    installed: &BTreeMap<String, InstalledAgent>,
    limits: HostReadLimits,
) -> Result<String, LiveHostError> {
    let binding = installation_digest(installed)?;
    let digest = cursor_digest(&session.opened_at, &session.session_id, &binding)?;
    let cursor = SessionCursorV1 {
        schema_version: 1,
        opened_at: session.opened_at.clone(),
        session_id: session.session_id.clone(),
        installation_binding_digest: binding,
        cursor_digest: digest,
    };
    let bytes = serde_jcs::to_vec(&cursor).map_err(|_| LiveHostError::CorruptState)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    if token.len() > limits.max_cursor_bytes {
        Err(LiveHostError::ReadBoundExceeded)
    } else {
        Ok(token)
    }
}

pub(super) fn decode(
    token: &str,
    installed: &BTreeMap<String, InstalledAgent>,
    limits: HostReadLimits,
) -> Result<(String, String), LiveHostError> {
    if token.is_empty() || token.len() > limits.max_cursor_bytes {
        return Err(LiveHostError::InvalidRequest);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| LiveHostError::InvalidRequest)?;
    if bytes.len() > limits.max_cursor_bytes {
        return Err(LiveHostError::InvalidRequest);
    }
    let cursor: SessionCursorV1 =
        serde_json::from_slice(&bytes).map_err(|_| LiveHostError::InvalidRequest)?;
    if serde_jcs::to_vec(&cursor).map_err(|_| LiveHostError::InvalidRequest)? != bytes
        || cursor.schema_version != 1
        || chrono::DateTime::parse_from_rfc3339(&cursor.opened_at).is_err()
        || cursor.installation_binding_digest != installation_digest(installed)?
        || cursor.cursor_digest
            != cursor_digest(
                &cursor.opened_at,
                &cursor.session_id,
                &cursor.installation_binding_digest,
            )?
    {
        return Err(LiveHostError::InvalidRequest);
    }
    Ok((cursor.opened_at, cursor.session_id))
}

fn installation_digest(
    installed: &BTreeMap<String, InstalledAgent>,
) -> Result<String, LiveHostError> {
    let bindings = installed
        .values()
        .map(|value| {
            json!({
                "definition_id": value.definition_id,
                "definition_revision": value.definition_revision,
                "snapshot_digest": value.snapshot_digest,
            })
        })
        .collect::<Vec<_>>();
    let bytes = serde_jcs::to_vec(&bindings).map_err(|_| LiveHostError::CorruptState)?;
    Ok(hex_digest(&bytes))
}

fn cursor_digest(
    opened_at: &str,
    session_id: &str,
    binding: &str,
) -> Result<String, LiveHostError> {
    let bytes = serde_jcs::to_vec(&CursorDigest {
        schema_version: 1,
        opened_at,
        session_id,
        installation_binding_digest: binding,
    })
    .map_err(|_| LiveHostError::CorruptState)?;
    Ok(hex_digest(&bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
