//! Runtime-owned commands and durable values for the M2 Memory control plane.

use std::collections::BTreeSet;

use garive_ledger::CommitResult;
use garive_ledger::{ExecutionId, SessionId, TurnId};
use garive_memory::DurableFactReference;
use garive_memory::{
    ContentBinding, MemoryAuthorizedScope, MemoryControlDocument, MemoryControlError,
    MemoryErasureTarget, MemoryImportOperation, MemoryImportPlan, MemoryRecordRef,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core_bridge::MemoryPrefix;

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

/// Product-visible readiness of one configured fact-backed Memory repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryRepositoryStatus {
    /// One verified namespace projection is ready for bounded reads.
    Ready {
        /// Exact opaque namespace identity.
        namespace_id: String,
        /// Current non-zero repository revision.
        repository_revision: u64,
    },
    /// No fact-backed repository is installed for the requested namespace.
    Unavailable,
    /// Durable facts and the current projection failed reconciliation.
    Corrupt,
}

/// Runtime-frozen authority and durable coordinates for one fact-backed M2 import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRepositoryImportContext {
    /// Session receiving the complete ordered fact batch.
    pub session_id: SessionId,
    /// Owning user-visible Turn.
    pub turn_id: TurnId,
    /// Owning disposable Execution.
    pub execution_id: ExecutionId,
    /// Exact optimistic Session version before import.
    pub expected_session_version: u64,
    /// Highest fixed fact position before import.
    pub through_position: u64,
    /// Canonical complete fixed prefixes used to recover repository pre-state.
    pub repository_prefixes: Vec<MemoryPrefix>,
    /// Canonical Runtime observation time.
    pub recorded_at: String,
    /// Verified user confirmation or approval fact inside the fixed prefix.
    pub authorization_fact: DurableFactReference,
    /// Verified receipt binding user-declared authority.
    pub authority_receipt_digest: String,
    /// Runtime-owned policy snapshot; never supplied by Desktop.
    pub policy: MemoryRepositoryImportPolicy,
}

/// Runtime-frozen physical erasure configuration for imports containing Erase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRepositoryErasurePolicy {
    /// Exact configured erasure-policy revision.
    pub policy_revision: String,
    /// Canonical configured storage targets.
    pub targets: Vec<MemoryErasureTarget>,
}

impl MemoryRepositoryErasurePolicy {
    /// Validates non-empty canonical target order and configured policy identity.
    pub fn new(
        policy_revision: impl Into<String>,
        targets: Vec<MemoryErasureTarget>,
    ) -> Result<Self, MemoryRepositoryError> {
        let value = Self {
            policy_revision: policy_revision.into(),
            targets,
        };
        if !valid_identity(&value.policy_revision)
            || value.targets.is_empty()
            || value.targets.len() > 64
            || !value.targets.windows(2).all(|pair| {
                pair[0].kind() < pair[1].kind()
                    || pair[0].kind() == pair[1].kind() && pair[0].target_id() < pair[1].target_id()
            })
        {
            return Err(MemoryRepositoryError::Unauthorized);
        }
        Ok(value)
    }
}

/// Complete Runtime policy snapshot governing one fact-backed M2 import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRepositoryImportPolicy {
    /// Runtime-selected retention policy.
    pub retention_policy_digest: String,
    /// Runtime-selected M1 classification policy revision.
    pub classification_policy_revision: String,
    /// Frozen M0 confidence metadata for explicit user declarations.
    pub user_declared_confidence_basis_points: u16,
    /// Platform-only aggregation policy, when Platform scope is admitted.
    pub platform_aggregation_policy_digest: Option<String>,
    /// Physical erasure configuration required only by Erase operations.
    pub erasure: Option<MemoryRepositoryErasurePolicy>,
}

impl MemoryRepositoryImportPolicy {
    /// Validates every configured policy binding without reading environment state.
    pub fn new(
        retention_policy_digest: impl Into<String>,
        classification_policy_revision: impl Into<String>,
        user_declared_confidence_basis_points: u16,
        platform_aggregation_policy_digest: Option<String>,
        erasure: Option<MemoryRepositoryErasurePolicy>,
    ) -> Result<Self, MemoryRepositoryError> {
        let value = Self {
            retention_policy_digest: retention_policy_digest.into(),
            classification_policy_revision: classification_policy_revision.into(),
            user_declared_confidence_basis_points,
            platform_aggregation_policy_digest,
            erasure,
        };
        if !valid_digest(&value.retention_policy_digest)
            || !valid_identity(&value.classification_policy_revision)
            || value.user_declared_confidence_basis_points > 10_000
            || value
                .platform_aggregation_policy_digest
                .as_deref()
                .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(MemoryRepositoryError::Unauthorized);
        }
        Ok(value)
    }
}

impl MemoryRepositoryImportContext {
    /// Validates fixed-prefix ownership and every explicit policy binding.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        turn_id: TurnId,
        execution_id: ExecutionId,
        expected_session_version: u64,
        through_position: u64,
        repository_prefixes: Vec<MemoryPrefix>,
        recorded_at: impl Into<String>,
        authorization_fact: DurableFactReference,
        authority_receipt_digest: impl Into<String>,
        policy: MemoryRepositoryImportPolicy,
    ) -> Result<Self, MemoryRepositoryError> {
        let value = Self {
            session_id,
            turn_id,
            execution_id,
            expected_session_version,
            through_position,
            repository_prefixes,
            recorded_at: recorded_at.into(),
            authorization_fact,
            authority_receipt_digest: authority_receipt_digest.into(),
            policy,
        };
        if value.expected_session_version == 0
            || value.through_position == 0
            || value.repository_prefixes.is_empty()
            || !value
                .repository_prefixes
                .windows(2)
                .all(|pair| pair[0].session_id < pair[1].session_id)
            || value
                .repository_prefixes
                .iter()
                .filter(|prefix| prefix.session_id == value.session_id)
                .count()
                != 1
            || value.repository_prefixes.iter().any(|prefix| {
                prefix.through_position == 0
                    || prefix.session_id == value.session_id
                        && prefix.through_position != value.through_position
            })
            || value.authorization_fact.session_id() != value.session_id.as_str()
            || value.authorization_fact.position() > value.through_position
            || !valid_digest(&value.authority_receipt_digest)
            || chrono::DateTime::parse_from_rfc3339(&value.recorded_at)
                .ok()
                .is_none_or(|time| {
                    time.with_timezone(&chrono::Utc)
                        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                        != value.recorded_at
                })
        {
            return Err(MemoryRepositoryError::Unauthorized);
        }
        Ok(value)
    }
}

/// Atomic result of one fact-backed Memory repository write or exact replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRepositoryCommitResult {
    /// Durable Ledger coordinates for the source fact batch.
    pub ledger: CommitResult,
    /// Repository revision observed before the original commit.
    pub previous_repository_revision: u64,
    /// Repository revision containing the committed Memory revision.
    pub committed_repository_revision: u64,
}

/// Stable production repository availability, integrity, concurrency, or authority failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryRepositoryError {
    /// The configured repository or required content resolver is unavailable.
    Unavailable,
    /// Source facts and the current projection cannot be reconciled.
    Corrupt,
    /// A fixed prefix or repository revision changed before commit.
    Stale,
    /// Namespace, scope, content, or action authority is absent.
    Unauthorized,
}

impl MemoryRepositoryError {
    /// Returns the accepted M2-C2 public failure code.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Unavailable => "memory_repository_unavailable",
            Self::Corrupt => "memory_repository_corrupt",
            Self::Stale => "memory_repository_stale",
            Self::Unauthorized => "memory_repository_unauthorized",
        }
    }
}

impl From<MemoryControlRuntimeError> for MemoryRepositoryError {
    fn from(value: MemoryControlRuntimeError) -> Self {
        match value {
            MemoryControlRuntimeError::PersistenceFailed => Self::Unavailable,
            MemoryControlRuntimeError::StaleSnapshot => Self::Stale,
            MemoryControlRuntimeError::Unauthorized => Self::Unauthorized,
            _ => Self::Corrupt,
        }
    }
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
    pub(crate) const fn max_id_bytes(&self) -> usize {
        self.max_id_bytes
    }
    pub(crate) fn document_for_operation(
        &self,
        operation: &MemoryImportOperation,
    ) -> Result<&MemoryControlDocument, MemoryControlRuntimeError> {
        let matches = self
            .documents
            .iter()
            .filter(|document| {
                let identity_matches = match (operation, document.record_ref()) {
                    (
                        MemoryImportOperation::Add {
                            source_draft_token, ..
                        },
                        MemoryRecordRef::New { draft_token },
                    ) => source_draft_token == draft_token,
                    (
                        MemoryImportOperation::Supersede {
                            record_id,
                            expected_active_revision_id,
                            authority,
                            ..
                        },
                        MemoryRecordRef::Existing {
                            record_id: document_record,
                            revision_id,
                        },
                    ) => {
                        record_id == document_record
                            && expected_active_revision_id == revision_id
                            && *authority == document.authority()
                            && !document.erase_requested()
                    }
                    (
                        MemoryImportOperation::Archive {
                            record_id,
                            expected_active_revision_id,
                            ..
                        },
                        MemoryRecordRef::Existing {
                            record_id: document_record,
                            revision_id,
                        },
                    ) => {
                        record_id == document_record
                            && expected_active_revision_id == revision_id
                            && document.lifecycle() == garive_memory::HypothesisState::Archived
                            && !document.erase_requested()
                    }
                    (
                        MemoryImportOperation::Erase {
                            record_id,
                            expected_active_revision_id,
                            ..
                        },
                        MemoryRecordRef::Existing {
                            record_id: document_record,
                            revision_id,
                        },
                    ) => {
                        record_id == document_record
                            && expected_active_revision_id == revision_id
                            && document.erase_requested()
                    }
                    _ => false,
                };
                identity_matches
                    && operation_document_digest(operation) == document.document_digest()
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            Ok(matches[0])
        } else {
            Err(MemoryControlRuntimeError::InvalidSnapshot)
        }
    }
}

fn operation_document_digest(operation: &MemoryImportOperation) -> &str {
    match operation {
        MemoryImportOperation::Add {
            document_digest, ..
        }
        | MemoryImportOperation::Supersede {
            document_digest, ..
        }
        | MemoryImportOperation::Archive {
            document_digest, ..
        }
        | MemoryImportOperation::Erase {
            document_digest, ..
        } => document_digest,
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
    /// The capability destination is unsafe, occupied, or incomplete.
    ExportTargetInvalid,
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
            Self::ExportTargetInvalid => "memory_export_target_invalid",
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

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
