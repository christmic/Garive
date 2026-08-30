//! Runtime-owned commands and durable values for the M2 Memory control plane.

use std::collections::BTreeSet;

use garive_memory::{
    ContentBinding, MemoryAuthorizedScope, MemoryControlDocument, MemoryControlError,
    MemoryImportPlan,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One Runtime authority operation admitted by an exact grant.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MemoryControlAction {
    /// Read and export the namespace projection.
    Export,
    /// Commit a previously prepared import plan.
    Import,
}

/// Exact namespace, action, and scope authority resolved before control work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryControlGrant {
    namespace_id: String,
    actions: BTreeSet<MemoryControlAction>,
    scopes: BTreeSet<MemoryAuthorizedScope>,
}

/// One verified fixed-revision namespace projection used by recall and export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryControlProjection {
    /// Exact Memory namespace.
    pub namespace_id: String,
    /// Non-zero optimistic repository revision.
    pub repository_revision: u64,
    /// Canonical current documents ordered by raw record identity.
    pub documents: Vec<MemoryControlDocument>,
}

impl MemoryControlGrant {
    /// Constructs a bounded exact grant without applying scope hierarchy.
    pub fn new(
        namespace_id: impl Into<String>,
        actions: impl IntoIterator<Item = MemoryControlAction>,
        scopes: impl IntoIterator<Item = MemoryAuthorizedScope>,
    ) -> Result<Self, MemoryControlRuntimeError> {
        let namespace_id = namespace_id.into();
        let actions: BTreeSet<_> = actions.into_iter().collect();
        let scopes: BTreeSet<_> = scopes.into_iter().collect();
        if !valid_identity(&namespace_id) || actions.is_empty() {
            return Err(MemoryControlRuntimeError::Unauthorized);
        }
        Ok(Self {
            namespace_id,
            actions,
            scopes,
        })
    }

    pub(crate) fn admits(
        &self,
        namespace_id: &str,
        action: MemoryControlAction,
        scope: &MemoryAuthorizedScope,
    ) -> bool {
        self.namespace_id == namespace_id
            && self.actions.contains(&action)
            && self.scopes.contains(scope)
    }

    pub(crate) fn admits_action(&self, namespace_id: &str, action: MemoryControlAction) -> bool {
        self.namespace_id == namespace_id && self.actions.contains(&action)
    }
}

/// One exact, idempotent M2 import commit request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryImportCommand {
    command_id: String,
    receipt_id: String,
    event_id: String,
    plan: MemoryImportPlan,
    documents: Vec<MemoryControlDocument>,
    max_id_bytes: usize,
}

impl MemoryImportCommand {
    /// Freezes command identities, the verified plan, and its edited documents.
    pub fn new(
        command_id: impl Into<String>,
        receipt_id: impl Into<String>,
        event_id: impl Into<String>,
        plan: MemoryImportPlan,
        documents: Vec<MemoryControlDocument>,
        max_id_bytes: usize,
    ) -> Result<Self, MemoryControlRuntimeError> {
        let value = Self {
            command_id: command_id.into(),
            receipt_id: receipt_id.into(),
            event_id: event_id.into(),
            plan,
            documents,
            max_id_bytes,
        };
        if !valid_identity(&value.command_id)
            || !valid_identity(&value.receipt_id)
            || !valid_identity(&value.event_id)
            || value.max_id_bytes == 0
        {
            return Err(MemoryControlRuntimeError::InvalidSnapshot);
        }
        value
            .plan
            .verify()
            .map_err(MemoryControlRuntimeError::from)?;
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
    pub(crate) const fn plan(&self) -> &MemoryImportPlan {
        &self.plan
    }
    pub(crate) fn documents(&self) -> &[MemoryControlDocument] {
        &self.documents
    }
    pub(crate) const fn max_id_bytes(&self) -> usize {
        self.max_id_bytes
    }
}

/// Canonical public result of one committed or replayed M2 import.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryImportReceipt {
    /// Exact v1 schema discriminator.
    pub schema_version: u8,
    /// Runtime-allocated receipt identity.
    pub receipt_id: String,
    /// Idempotent caller command identity.
    pub command_id: String,
    /// Snapshot export identity bound by the plan.
    pub export_id: String,
    /// Exact Memory namespace.
    pub namespace_id: String,
    /// Canonical plan digest.
    pub plan_digest: String,
    /// Repository revision observed before commit.
    pub previous_repository_revision: u64,
    /// Repository revision after commit.
    pub committed_repository_revision: u64,
    /// Number of added records.
    pub add_count: u64,
    /// Number of superseding revisions.
    pub supersede_count: u64,
    /// Number of lifecycle archives.
    pub archive_count: u64,
    /// Number of explicit erasures.
    pub erase_count: u64,
    /// Whether the repository revision advanced.
    pub changed: bool,
    /// Lowercase SHA-256 over JCS receipt fields excluding this field.
    pub receipt_digest: String,
}

impl MemoryImportReceipt {
    pub(crate) fn create(
        command: &MemoryImportCommand,
        previous_repository_revision: u64,
        committed_repository_revision: u64,
    ) -> Result<(Self, String), MemoryControlRuntimeError> {
        let plan = command.plan();
        let changed = !plan.operations.is_empty();
        let preimage = MemoryImportReceiptPreimage {
            schema_version: 1,
            receipt_id: command.receipt_id(),
            command_id: command.command_id(),
            export_id: &plan.export_id,
            namespace_id: &plan.namespace_id,
            plan_digest: &plan.plan_digest,
            previous_repository_revision,
            committed_repository_revision,
            add_count: plan.add_count,
            supersede_count: plan.supersede_count,
            archive_count: plan.archive_count,
            erase_count: plan.erase_count,
            changed,
        };
        let (_, receipt_digest) = canonical_digest(&preimage)?;
        let receipt = Self {
            schema_version: 1,
            receipt_id: command.receipt_id().to_owned(),
            command_id: command.command_id().to_owned(),
            export_id: plan.export_id.clone(),
            namespace_id: plan.namespace_id.clone(),
            plan_digest: plan.plan_digest.clone(),
            previous_repository_revision,
            committed_repository_revision,
            add_count: plan.add_count,
            supersede_count: plan.supersede_count,
            archive_count: plan.archive_count,
            erase_count: plan.erase_count,
            changed,
            receipt_digest,
        };
        let (json, _) = canonical_digest(&receipt)?;
        Ok((receipt, json))
    }

    pub(crate) fn decode_verified(json: &str) -> Result<Self, MemoryControlRuntimeError> {
        let receipt: Self =
            serde_json::from_str(json).map_err(|_| MemoryControlRuntimeError::PersistenceFailed)?;
        let preimage = receipt.preimage();
        let (canonical_preimage, digest) = canonical_digest(&preimage)?;
        let (canonical_receipt, _) = canonical_digest(&receipt)?;
        let expected_changed = receipt.add_count
            + receipt.supersede_count
            + receipt.archive_count
            + receipt.erase_count
            > 0;
        let revision_valid = if expected_changed {
            receipt
                .previous_repository_revision
                .checked_add(1)
                .is_some_and(|value| value == receipt.committed_repository_revision)
        } else {
            receipt.previous_repository_revision == receipt.committed_repository_revision
        };
        if canonical_preimage.is_empty()
            || digest != receipt.receipt_digest
            || canonical_receipt != json
            || receipt.schema_version != 1
            || receipt.previous_repository_revision == 0
            || receipt.changed != expected_changed
            || !revision_valid
        {
            return Err(MemoryControlRuntimeError::PersistenceFailed);
        }
        Ok(receipt)
    }

    fn preimage(&self) -> MemoryImportReceiptPreimage<'_> {
        MemoryImportReceiptPreimage {
            schema_version: self.schema_version,
            receipt_id: &self.receipt_id,
            command_id: &self.command_id,
            export_id: &self.export_id,
            namespace_id: &self.namespace_id,
            plan_digest: &self.plan_digest,
            previous_repository_revision: self.previous_repository_revision,
            committed_repository_revision: self.committed_repository_revision,
            add_count: self.add_count,
            supersede_count: self.supersede_count,
            archive_count: self.archive_count,
            erase_count: self.erase_count,
            changed: self.changed,
        }
    }
}

#[derive(Serialize)]
struct MemoryImportReceiptPreimage<'a> {
    schema_version: u8,
    receipt_id: &'a str,
    command_id: &'a str,
    export_id: &'a str,
    namespace_id: &'a str,
    plan_digest: &'a str,
    previous_repository_revision: u64,
    committed_repository_revision: u64,
    add_count: u64,
    supersede_count: u64,
    archive_count: u64,
    erase_count: u64,
    changed: bool,
}

#[derive(Serialize)]
pub(crate) struct MemoryImportJournalEvent<'a> {
    pub(crate) schema_version: u8,
    pub(crate) event_id: &'a str,
    pub(crate) namespace_id: &'a str,
    pub(crate) command_id: &'a str,
    pub(crate) plan_digest: &'a str,
    pub(crate) previous_repository_revision: u64,
    pub(crate) committed_repository_revision: u64,
    pub(crate) operations: &'a ContentBinding,
    pub(crate) receipt_digest: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) event_digest: Option<&'a str>,
}

/// Stable Runtime failure for M2 authorization, validation, conflict, or durability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryControlRuntimeError {
    /// Exact namespace, action, or scope authority was absent.
    Unauthorized,
    /// Snapshot, command, or persisted canonical value was invalid.
    InvalidSnapshot,
    /// A declared control-plane bound was exceeded.
    BoundExceeded,
    /// The import attempted a forbidden authority or metadata change.
    ForbiddenChange,
    /// Repository or affected revision no longer matched the plan.
    StaleSnapshot,
    /// One command identity was reused with different semantics.
    CommandConflict,
    /// SQLite could not atomically persist or verify the command.
    PersistenceFailed,
}

impl MemoryControlRuntimeError {
    /// Returns the exact stable public error code.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Unauthorized => "memory_control_unauthorized",
            Self::InvalidSnapshot => "memory_snapshot_invalid",
            Self::BoundExceeded => "memory_control_bound_exceeded",
            Self::ForbiddenChange => "memory_import_forbidden_change",
            Self::StaleSnapshot => "stale_memory_snapshot",
            Self::CommandConflict => "memory_import_command_conflict",
            Self::PersistenceFailed => "memory_control_persistence_failed",
        }
    }
}

impl From<MemoryControlError> for MemoryControlRuntimeError {
    fn from(value: MemoryControlError) -> Self {
        match value {
            MemoryControlError::InvalidLimits | MemoryControlError::InvalidSnapshot => {
                Self::InvalidSnapshot
            }
            MemoryControlError::BoundExceeded => Self::BoundExceeded,
            MemoryControlError::ForbiddenChange => Self::ForbiddenChange,
            MemoryControlError::StaleSnapshot => Self::StaleSnapshot,
        }
    }
}

pub(crate) fn canonical_digest<T: Serialize>(
    value: &T,
) -> Result<(String, String), MemoryControlRuntimeError> {
    let json =
        serde_jcs::to_string(value).map_err(|_| MemoryControlRuntimeError::InvalidSnapshot)?;
    let digest = hex_sha256(json.as_bytes());
    Ok((json, digest))
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.trim() == value
}
