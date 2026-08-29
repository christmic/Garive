use crate::{
    values::{valid_digest, valid_id, valid_text, MAX_REFERENCE_BYTES},
    DurableFactReference, MemoryError, MemoryErrorCode,
};

const MAX_ERASURE_TARGETS: usize = 64;

/// Runtime-owned storage class participating in physical erasure.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ErasureTargetKind {
    /// Authoritative content store.
    PrimaryStore,
    /// Rebuildable query or search projection.
    Projection,
    /// Ephemeral cache with explicit deletion authority.
    Cache,
    /// Backup subject to an explicit retention window.
    Backup,
}

/// One canonical configured erasure target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryErasureTarget {
    target_id: String,
    kind: ErasureTargetKind,
}

impl MemoryErasureTarget {
    /// Validates an opaque configured target identity.
    pub fn new(target_id: impl Into<String>, kind: ErasureTargetKind) -> Result<Self, MemoryError> {
        let value = Self {
            target_id: target_id.into(),
            kind,
        };
        if !valid_id(&value.target_id) {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the configured target identity.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
    /// Returns the configured storage class.
    pub const fn kind(&self) -> ErasureTargetKind {
        self.kind
    }
}

/// Physical erasure request admitted only after an exact logical tombstone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryErasureRequest {
    request_id: String,
    namespace_id: String,
    record_id: String,
    revision_id: String,
    tombstone_fact: DurableFactReference,
    policy_revision: String,
    targets: Vec<MemoryErasureTarget>,
}

impl MemoryErasureRequest {
    /// Validates exact target identity, canonical ordering, and tombstone binding shape.
    pub fn new(
        request_id: impl Into<String>,
        namespace_id: impl Into<String>,
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        tombstone_fact: DurableFactReference,
        policy_revision: impl Into<String>,
        targets: Vec<MemoryErasureTarget>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            request_id: request_id.into(),
            namespace_id: namespace_id.into(),
            record_id: record_id.into(),
            revision_id: revision_id.into(),
            tombstone_fact,
            policy_revision: policy_revision.into(),
            targets,
        };
        if [
            &value.request_id,
            &value.namespace_id,
            &value.record_id,
            &value.revision_id,
        ]
        .iter()
        .any(|item| !valid_id(item))
            || !valid_text(&value.policy_revision, MAX_REFERENCE_BYTES)
            || value.targets.is_empty()
            || value.targets.len() > MAX_ERASURE_TARGETS
            || !value.targets.windows(2).all(|pair| {
                pair[0].kind < pair[1].kind
                    || pair[0].kind == pair[1].kind && pair[0].target_id < pair[1].target_id
            })
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the erasure request identity.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// Returns the authorized namespace.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the logical record identity.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the immutable revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the exact tombstone fact binding.
    pub const fn tombstone_fact(&self) -> &DurableFactReference {
        &self.tombstone_fact
    }
    /// Returns the configured erasure policy revision.
    pub fn policy_revision(&self) -> &str {
        &self.policy_revision
    }
    /// Returns canonical configured targets.
    pub fn targets(&self) -> &[MemoryErasureTarget] {
        &self.targets
    }
}

/// Per-target physical erasure outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErasureTargetStatus {
    /// Content was physically erased.
    Erased,
    /// Target proved that the content was absent.
    NotPresent,
    /// Backup is retained until the reported later position.
    PendingBackupRetention,
    /// A later Runtime retry is required.
    PendingRetry,
}

/// Receipt-shaped result for one configured target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryErasureTargetResult {
    target_id: String,
    status: ErasureTargetStatus,
    receipt_digest: String,
    not_before_position: Option<u64>,
}

impl MemoryErasureTargetResult {
    /// Validates the target identity, receipt digest, and optional position shape.
    pub fn new(
        target_id: impl Into<String>,
        status: ErasureTargetStatus,
        receipt_digest: impl Into<String>,
        not_before_position: Option<u64>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            target_id: target_id.into(),
            status,
            receipt_digest: receipt_digest.into(),
            not_before_position,
        };
        if !valid_id(&value.target_id)
            || !valid_digest(&value.receipt_digest)
            || (status == ErasureTargetStatus::PendingBackupRetention)
                != not_before_position.is_some()
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the configured target identity.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
    /// Returns the exact physical outcome.
    pub const fn status(&self) -> ErasureTargetStatus {
        self.status
    }
    /// Returns the target-operation receipt digest.
    pub fn receipt_digest(&self) -> &str {
        &self.receipt_digest
    }
    /// Returns the backup retention boundary when pending.
    pub const fn not_before_position(&self) -> Option<u64> {
        self.not_before_position
    }
}

/// Aggregate erasure state derived from every configured target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErasureDisposition {
    /// Every target is erased or proved absent.
    Complete,
    /// At least one target remains pending.
    Partial,
}

/// Immutable result of one complete target-coverage attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryErasureReceipt {
    request_id: String,
    attempt_id: String,
    attempted_at_position: u64,
    results: Vec<MemoryErasureTargetResult>,
    disposition: ErasureDisposition,
}

impl MemoryErasureReceipt {
    /// Returns the originating request identity.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// Returns the attempt identity.
    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }
    /// Returns the durable attempt position.
    pub const fn attempted_at_position(&self) -> u64 {
        self.attempted_at_position
    }
    /// Returns every result in configured target order.
    pub fn results(&self) -> &[MemoryErasureTargetResult] {
        &self.results
    }
    /// Returns whether all configured targets are complete.
    pub const fn disposition(&self) -> ErasureDisposition {
        self.disposition
    }
}

/// Validates exact target coverage and derives Complete versus Partial.
pub fn record_memory_erasure(
    request: &MemoryErasureRequest,
    attempt_id: impl Into<String>,
    attempted_at_position: u64,
    results: Vec<MemoryErasureTargetResult>,
) -> Result<MemoryErasureReceipt, MemoryError> {
    let attempt_id = attempt_id.into();
    if !valid_id(&attempt_id)
        || attempted_at_position <= request.tombstone_fact.position()
        || results.len() != request.targets.len()
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    for (target, result) in request.targets.iter().zip(&results) {
        if target.target_id != result.target_id
            || result.status == ErasureTargetStatus::PendingBackupRetention
                && (target.kind != ErasureTargetKind::Backup
                    || match result.not_before_position {
                        Some(position) => position <= attempted_at_position,
                        None => true,
                    })
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
    }
    let disposition = if results.iter().all(|result| {
        matches!(
            result.status,
            ErasureTargetStatus::Erased | ErasureTargetStatus::NotPresent
        )
    }) {
        ErasureDisposition::Complete
    } else {
        ErasureDisposition::Partial
    };
    Ok(MemoryErasureReceipt {
        request_id: request.request_id.clone(),
        attempt_id,
        attempted_at_position,
        results,
        disposition,
    })
}
