use std::collections::{BTreeMap, BTreeSet};

use garive_memory::{DistillationWatermark, ErasureDisposition};

/// Redacted committed maintenance decision rebuilt from the ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedMemoryDecision {
    /// Candidate identity.
    pub candidate_id: String,
    /// Exact four-way decision kind.
    pub decision_kind: String,
}

/// Latest physical erasure status for one logical request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedMemoryErasure {
    /// Erasure request identity.
    pub request_id: String,
    /// Latest attempt identity.
    pub attempt_id: String,
    /// Durable position reported by the latest attempt.
    pub attempted_at_position: u64,
    /// Complete only when every configured target finished.
    pub disposition: ErasureDisposition,
    /// Targets still awaiting retention or retry.
    pub pending_targets: Vec<String>,
}

/// Namespace-isolated maintenance state reconstructed from durable facts only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryMaintenanceProjection {
    pub(super) namespace_id: String,
    pub(super) decisions: Vec<RecordedMemoryDecision>,
    pub(super) watermarks: BTreeMap<(String, String), DistillationWatermark>,
    pub(super) promoted: BTreeSet<(String, String)>,
    pub(super) erasures: BTreeMap<String, RecordedMemoryErasure>,
    pub(super) audit_count: u64,
}

impl MemoryMaintenanceProjection {
    /// Returns the authorized namespace.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns committed candidate decisions in prefix order.
    pub fn decisions(&self) -> &[RecordedMemoryDecision] {
        &self.decisions
    }
    /// Returns the latest checkpoint for an extractor and source Session.
    pub fn watermark(&self, extractor: &str, session: &str) -> Option<&DistillationWatermark> {
        self.watermarks.get(&(extractor.into(), session.into()))
    }
    /// Tests whether a revision has a receipt-backed Promoted transition.
    pub fn is_promoted(&self, record_id: &str, revision_id: &str) -> bool {
        self.promoted
            .contains(&(record_id.into(), revision_id.into()))
    }
    /// Returns the latest erasure attempt for one request.
    pub fn erasure(&self, request_id: &str) -> Option<&RecordedMemoryErasure> {
        self.erasures.get(request_id)
    }
    /// Returns the count of committed bounded audit reports.
    pub const fn audit_count(&self) -> u64 {
        self.audit_count
    }
}
