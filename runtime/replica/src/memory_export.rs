//! Canonical Runtime values for recoverable M2 snapshot export.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    memory_control::{canonical_digest, hex_sha256},
    MemoryControlRuntimeError,
};

/// One destination resolved by an authorized Runtime file capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryExportTarget {
    path: PathBuf,
    capability_binding_digest: String,
}

impl MemoryExportTarget {
    /// Freezes an already authorized destination and its secret-free capability binding.
    ///
    /// Callers must resolve user-selected paths through the Runtime capability layer
    /// before constructing this value; Host and App APIs expose only opaque capability IDs.
    pub fn authorized(
        path: impl Into<PathBuf>,
        capability_binding_digest: impl Into<String>,
    ) -> Result<Self, MemoryControlRuntimeError> {
        let value = Self {
            path: path.into(),
            capability_binding_digest: capability_binding_digest.into(),
        };
        if !valid_digest(&value.capability_binding_digest)
            || !value.path.is_absolute()
            || value.path.file_name().is_none()
            || value.path.parent().is_none()
        {
            return Err(MemoryControlRuntimeError::ExportTargetInvalid);
        }
        Ok(value)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn capability_binding_digest(&self) -> &str {
        &self.capability_binding_digest
    }
}

/// One exact idempotent request to export a fixed Memory repository revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryExportCommand {
    command_id: String,
    receipt_id: String,
    event_id: String,
    export_id: String,
    namespace_id: String,
    exported_at: String,
}

impl MemoryExportCommand {
    /// Freezes all Runtime-generated export identities and display time.
    pub fn new(
        command_id: impl Into<String>,
        receipt_id: impl Into<String>,
        event_id: impl Into<String>,
        export_id: impl Into<String>,
        namespace_id: impl Into<String>,
        exported_at: impl Into<String>,
    ) -> Result<Self, MemoryControlRuntimeError> {
        let value = Self {
            command_id: command_id.into(),
            receipt_id: receipt_id.into(),
            event_id: event_id.into(),
            export_id: export_id.into(),
            namespace_id: namespace_id.into(),
            exported_at: exported_at.into(),
        };
        if [
            value.command_id.as_str(),
            value.receipt_id.as_str(),
            value.event_id.as_str(),
            value.export_id.as_str(),
            value.namespace_id.as_str(),
        ]
        .into_iter()
        .any(|identity| identity.is_empty() || identity.len() > 128 || identity.trim() != identity)
            || chrono::DateTime::parse_from_rfc3339(&value.exported_at).is_err()
        {
            return Err(MemoryControlRuntimeError::InvalidSnapshot);
        }
        Ok(value)
    }

    pub(crate) fn command_id(&self) -> &str {
        &self.command_id
    }
    pub(crate) fn receipt_id(&self) -> &str {
        &self.receipt_id
    }
    pub(crate) fn event_id(&self) -> &str {
        &self.event_id
    }
    pub(crate) fn export_id(&self) -> &str {
        &self.export_id
    }
    pub(crate) fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    pub(crate) fn exported_at(&self) -> &str {
        &self.exported_at
    }
}

/// Canonical public result of one committed or recovered M2 export.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryExportReceipt {
    /// Exact v1 schema discriminator.
    pub schema_version: u8,
    /// Runtime-allocated receipt identity.
    pub receipt_id: String,
    /// Idempotent caller command identity.
    pub command_id: String,
    /// Exact exported snapshot identity.
    pub export_id: String,
    /// Exact Memory namespace.
    pub namespace_id: String,
    /// Canonical manifest digest.
    pub manifest_digest: String,
    /// Fixed repository revision exported.
    pub through_repository_revision: u64,
    /// Number of exported current entries.
    pub entry_count: u64,
    /// Lowercase SHA-256 over JCS fields excluding this field.
    pub receipt_digest: String,
}

impl MemoryExportReceipt {
    pub(crate) fn create(
        command: &MemoryExportCommand,
        manifest_digest: &str,
        through_repository_revision: u64,
        entry_count: u64,
    ) -> Result<(Self, String), MemoryControlRuntimeError> {
        let preimage = MemoryExportReceiptPreimage {
            schema_version: 1,
            receipt_id: command.receipt_id(),
            command_id: command.command_id(),
            export_id: command.export_id(),
            namespace_id: command.namespace_id(),
            manifest_digest,
            through_repository_revision,
            entry_count,
        };
        let (_, receipt_digest) = canonical_digest(&preimage)?;
        let receipt = Self {
            schema_version: 1,
            receipt_id: command.receipt_id().to_owned(),
            command_id: command.command_id().to_owned(),
            export_id: command.export_id().to_owned(),
            namespace_id: command.namespace_id().to_owned(),
            manifest_digest: manifest_digest.to_owned(),
            through_repository_revision,
            entry_count,
            receipt_digest,
        };
        let (json, _) = canonical_digest(&receipt)?;
        Ok((receipt, json))
    }

    pub(crate) fn decode_verified(json: &str) -> Result<Self, MemoryControlRuntimeError> {
        let receipt: Self =
            serde_json::from_str(json).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let preimage = receipt.preimage();
        let (_, digest) = canonical_digest(&preimage)?;
        let (canonical, _) = canonical_digest(&receipt)?;
        if receipt.schema_version != 1
            || receipt.through_repository_revision == 0
            || !valid_digest(&receipt.manifest_digest)
            || digest != receipt.receipt_digest
            || canonical != json
        {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
        Ok(receipt)
    }

    fn preimage(&self) -> MemoryExportReceiptPreimage<'_> {
        MemoryExportReceiptPreimage {
            schema_version: self.schema_version,
            receipt_id: &self.receipt_id,
            command_id: &self.command_id,
            export_id: &self.export_id,
            namespace_id: &self.namespace_id,
            manifest_digest: &self.manifest_digest,
            through_repository_revision: self.through_repository_revision,
            entry_count: self.entry_count,
        }
    }
}

#[derive(Serialize)]
struct MemoryExportReceiptPreimage<'a> {
    schema_version: u8,
    receipt_id: &'a str,
    command_id: &'a str,
    export_id: &'a str,
    namespace_id: &'a str,
    manifest_digest: &'a str,
    through_repository_revision: u64,
    entry_count: u64,
}

#[derive(Serialize)]
pub(crate) struct MemoryExportJournalEvent<'a> {
    pub(crate) schema_version: u8,
    pub(crate) event_id: &'a str,
    pub(crate) namespace_id: &'a str,
    pub(crate) command_id: &'a str,
    pub(crate) export_id: &'a str,
    pub(crate) manifest_digest: &'a str,
    pub(crate) through_repository_revision: u64,
    pub(crate) receipt_digest: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) event_digest: Option<&'a str>,
}

pub(crate) fn export_binding_digest(
    command: &MemoryExportCommand,
    target: &MemoryExportTarget,
    manifest_digest: &str,
) -> Result<String, MemoryControlRuntimeError> {
    #[derive(Serialize)]
    struct Binding<'a> {
        schema_version: u8,
        command_id: &'a str,
        export_id: &'a str,
        namespace_id: &'a str,
        manifest_digest: &'a str,
        capability_binding_digest: &'a str,
    }
    let (json, _) = canonical_digest(&Binding {
        schema_version: 1,
        command_id: command.command_id(),
        export_id: command.export_id(),
        namespace_id: command.namespace_id(),
        manifest_digest,
        capability_binding_digest: target.capability_binding_digest(),
    })?;
    Ok(hex_sha256(json.as_bytes()))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
