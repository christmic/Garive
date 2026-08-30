//! Public values for pure M2 import planning.

use serde::Serialize;

use crate::{
    control_plane::hex_sha256, HypothesisState, MemoryAuthority, MemoryKind, MemoryScopeClass,
    MemorySensitivity, MemoryType,
};

/// Current Runtime-owned Memory projection row supplied to the pure planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCurrentEntry {
    /// Exact M0 record identity.
    pub record_id: String,
    /// Exact active M0 revision identity.
    pub revision_id: String,
    /// Frozen authority of the active revision.
    pub authority: MemoryAuthority,
    /// Orthogonal M1 type.
    pub memory_type: MemoryType,
    /// Preserved M0 role.
    pub memory_role: MemoryKind,
    /// Exact scope class.
    pub scope: MemoryScopeClass,
    /// Exact Runtime-authorized owner identity.
    pub scope_owner_id: String,
    /// Current M1 lifecycle.
    pub lifecycle: HypothesisState,
    /// Frozen sensitivity.
    pub sensitivity: MemorySensitivity,
    /// Digest of normalized current content.
    pub content_digest: String,
}

/// One exact scope binding admitted for new documents.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MemoryAuthorizedScope {
    /// Admitted scope class.
    pub scope: MemoryScopeClass,
    /// Exact admitted owner identity.
    pub owner_id: String,
}

/// Runtime-generated identities frozen before plan presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryIdentityAllocation {
    /// Identities for a package-local new document.
    Add {
        /// Exact draft token being allocated.
        draft_token: String,
        /// Fresh M0 record identity.
        record_id: String,
        /// Fresh first revision identity.
        revision_id: String,
    },
    /// Fresh revision identity for an existing record edit.
    Supersede {
        /// Existing M0 record identity.
        record_id: String,
        /// Fresh M0 revision identity.
        revision_id: String,
    },
}

/// One canonical M2 import operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MemoryImportOperation {
    /// Add one newly allocated user-declared record.
    Add {
        /// Package-local source token.
        source_draft_token: String,
        /// Fresh record identity.
        record_id: String,
        /// Fresh revision identity.
        revision_id: String,
        /// Absence precondition, always true in v1.
        expected_absent: bool,
        /// Canonical edited document digest.
        document_digest: String,
    },
    /// Create a new immutable revision under an existing record.
    Supersede {
        /// Existing record identity.
        record_id: String,
        /// Expected current revision.
        expected_active_revision_id: String,
        /// Fresh revision identity.
        new_revision_id: String,
        /// Authority of the new revision.
        authority: MemoryAuthority,
        /// Canonical edited document digest.
        document_digest: String,
        /// Learned revision retained as provenance when applicable.
        #[serde(skip_serializing_if = "Option::is_none")]
        supersedes_learned_revision_id: Option<String>,
    },
    /// Archive an existing current revision.
    Archive {
        /// Existing record identity.
        record_id: String,
        /// Expected current revision.
        expected_active_revision_id: String,
        /// Canonical edited document digest.
        document_digest: String,
    },
    /// Erase an existing current revision through M1 erasure.
    Erase {
        /// Existing record identity.
        record_id: String,
        /// Expected current revision.
        expected_active_revision_id: String,
        /// Canonical edited document digest.
        document_digest: String,
    },
}

impl MemoryImportOperation {
    /// Returns the exact record identity targeted by this operation.
    pub fn record_id(&self) -> &str {
        match self {
            Self::Add { record_id, .. }
            | Self::Supersede { record_id, .. }
            | Self::Archive { record_id, .. }
            | Self::Erase { record_id, .. } => record_id,
        }
    }
    pub(crate) const fn rank(&self) -> u8 {
        match self {
            Self::Add { .. } => 0,
            Self::Supersede { .. } => 1,
            Self::Archive { .. } => 2,
            Self::Erase { .. } => 3,
        }
    }
}

/// Canonical pure M2 import plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryImportPlan {
    /// Exact export identity.
    pub export_id: String,
    /// Exact Memory namespace.
    pub namespace_id: String,
    /// Repository revision captured by export.
    pub through_revision: u64,
    /// Verified input manifest digest.
    pub input_manifest_digest: String,
    /// Repository revision required at commit.
    pub expected_repository_revision: u64,
    /// Canonically ordered operations.
    pub operations: Vec<MemoryImportOperation>,
    /// Exact add count.
    pub add_count: u64,
    /// Exact supersede count.
    pub supersede_count: u64,
    /// Exact archive count.
    pub archive_count: u64,
    /// Exact erase count.
    pub erase_count: u64,
    /// Lowercase SHA-256 over the JCS plan without this field.
    pub plan_digest: String,
}

impl MemoryImportPlan {
    /// Verifies counts, canonical operation order, uniqueness, and the JCS digest.
    pub fn verify(&self) -> Result<(), crate::MemoryControlError> {
        let counts = (
            self.operations
                .iter()
                .filter(|v| matches!(v, MemoryImportOperation::Add { .. }))
                .count() as u64,
            self.operations
                .iter()
                .filter(|v| matches!(v, MemoryImportOperation::Supersede { .. }))
                .count() as u64,
            self.operations
                .iter()
                .filter(|v| matches!(v, MemoryImportOperation::Archive { .. }))
                .count() as u64,
            self.operations
                .iter()
                .filter(|v| matches!(v, MemoryImportOperation::Erase { .. }))
                .count() as u64,
        );
        let ordered = self
            .operations
            .windows(2)
            .all(|pair| pair[0].record_id() < pair[1].record_id());
        if !valid_identity(&self.export_id)
            || !valid_identity(&self.namespace_id)
            || self.through_revision == 0
            || self.expected_repository_revision == 0
            || !valid_sha256(&self.input_manifest_digest)
            || !valid_sha256(&self.plan_digest)
            || self.operations.iter().any(|operation| !operation.valid())
            || !ordered
            || counts
                != (
                    self.add_count,
                    self.supersede_count,
                    self.archive_count,
                    self.erase_count,
                )
        {
            return Err(crate::MemoryControlError::InvalidSnapshot);
        }
        let canonical = serde_jcs::to_vec(&self.preimage())
            .map_err(|_| crate::MemoryControlError::InvalidSnapshot)?;
        if hex_sha256(&canonical) != self.plan_digest {
            return Err(crate::MemoryControlError::InvalidSnapshot);
        }
        Ok(())
    }

    /// Returns exact canonical operation JSON for the durable M2 journal binding.
    pub fn canonical_operations_json(&self) -> Result<String, crate::MemoryControlError> {
        self.verify()?;
        serde_jcs::to_string(&self.operations)
            .map_err(|_| crate::MemoryControlError::InvalidSnapshot)
    }

    fn preimage(&self) -> PlanPreimage<'_> {
        PlanPreimage {
            schema_version: 1,
            export_id: &self.export_id,
            namespace_id: &self.namespace_id,
            through_revision: self.through_revision,
            input_manifest_digest: &self.input_manifest_digest,
            expected_repository_revision: self.expected_repository_revision,
            operations: &self.operations,
            add_count: self.add_count,
            supersede_count: self.supersede_count,
            archive_count: self.archive_count,
            erase_count: self.erase_count,
        }
    }
}

impl MemoryImportOperation {
    fn valid(&self) -> bool {
        match self {
            Self::Add {
                source_draft_token,
                record_id,
                revision_id,
                expected_absent,
                document_digest,
            } => {
                valid_identity(source_draft_token)
                    && valid_identity(record_id)
                    && valid_identity(revision_id)
                    && *expected_absent
                    && valid_sha256(document_digest)
            }
            Self::Supersede {
                record_id,
                expected_active_revision_id,
                new_revision_id,
                document_digest,
                supersedes_learned_revision_id,
                ..
            } => {
                valid_identity(record_id)
                    && valid_identity(expected_active_revision_id)
                    && valid_identity(new_revision_id)
                    && valid_sha256(document_digest)
                    && supersedes_learned_revision_id
                        .as_ref()
                        .is_none_or(|revision| valid_identity(revision))
            }
            Self::Archive {
                record_id,
                expected_active_revision_id,
                document_digest,
            }
            | Self::Erase {
                record_id,
                expected_active_revision_id,
                document_digest,
            } => {
                valid_identity(record_id)
                    && valid_identity(expected_active_revision_id)
                    && valid_sha256(document_digest)
            }
        }
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.trim() == value
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Serialize)]
pub(crate) struct PlanPreimage<'a> {
    pub(crate) schema_version: u8,
    pub(crate) export_id: &'a str,
    pub(crate) namespace_id: &'a str,
    pub(crate) through_revision: u64,
    pub(crate) input_manifest_digest: &'a str,
    pub(crate) expected_repository_revision: u64,
    pub(crate) operations: &'a [MemoryImportOperation],
    pub(crate) add_count: u64,
    pub(crate) supersede_count: u64,
    pub(crate) archive_count: u64,
    pub(crate) erase_count: u64,
}
