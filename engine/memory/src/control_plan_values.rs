//! Public values for pure M2 import planning.

use serde::Serialize;

use crate::{
    HypothesisState, MemoryAuthority, MemoryKind, MemoryScopeClass, MemorySensitivity, MemoryType,
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
    pub(crate) fn record_id(&self) -> &str {
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
