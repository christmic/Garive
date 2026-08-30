//! Public values for canonical M2 snapshot packages.

use serde::{Deserialize, Serialize};

use crate::{MemoryControlDocument, MemoryDocumentLimits};

/// Complete non-zero bounds for one in-memory snapshot package validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemorySnapshotLimits {
    /// Maximum exported plus new documents.
    pub max_entries: usize,
    /// Maximum manifest and document bytes combined.
    pub max_total_bytes: usize,
    /// Per-document limits.
    pub document: MemoryDocumentLimits,
}

impl MemorySnapshotLimits {
    /// Constructs non-zero package bounds.
    pub const fn new(
        max_entries: usize,
        max_total_bytes: usize,
        document: MemoryDocumentLimits,
    ) -> Result<Self, crate::MemoryControlError> {
        if max_entries == 0 || max_total_bytes == 0 {
            Err(crate::MemoryControlError::InvalidLimits)
        } else {
            Ok(Self {
                max_entries,
                max_total_bytes,
                document,
            })
        }
    }
}

/// Runtime-observed file input for the pure package validator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySnapshotFile {
    /// Exact package-relative file name.
    pub file_name: String,
    /// File bytes read under Runtime bounds.
    pub bytes: Vec<u8>,
    /// Runtime file identity used to reject hard-link aliases.
    pub storage_identity: String,
    /// True only for a regular file that was not reached through a symlink.
    pub regular: bool,
}

/// One exact exported manifest entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySnapshotEntry {
    /// Exact M0 record identity.
    pub record_id: String,
    /// Exact current M0 revision identity.
    pub revision_id: String,
    /// Canonical package-relative Markdown file.
    pub file_name: String,
    /// Frozen authority name.
    pub authority: String,
    /// Frozen M1 type name.
    pub memory_type: String,
    /// Preserved M0 role name.
    pub memory_role: String,
    /// Frozen scope class name.
    pub scope: String,
    /// Exact decoded scope owner identity.
    pub scope_owner_id: String,
    /// Current lifecycle name.
    pub lifecycle: String,
    /// Frozen sensitivity name.
    pub sensitivity: String,
    /// Normalized content digest.
    pub content_digest: String,
    /// Canonical Markdown digest.
    pub document_digest: String,
}

/// Canonical M2 snapshot manifest v1.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySnapshotManifest {
    /// Exact schema version, always one.
    pub schema_version: u8,
    /// Runtime-generated export identity.
    pub export_id: String,
    /// Exact Memory namespace.
    pub namespace_id: String,
    /// Non-zero repository revision captured by export.
    pub through_revision: u64,
    /// RFC 3339 display timestamp.
    pub exported_at: String,
    /// Canonically ordered current entries.
    pub entries: Vec<MemorySnapshotEntry>,
    /// SHA-256 over JCS manifest with this field omitted.
    pub manifest_digest: String,
}

/// One generated or verified M2 snapshot package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySnapshot {
    /// Parsed and digest-verified manifest.
    pub manifest: MemorySnapshotManifest,
    /// Exact canonical manifest JSON bytes.
    pub manifest_json: Vec<u8>,
    /// Exported and user-created documents in file-name order.
    pub documents: Vec<(String, MemoryControlDocument)>,
}
