//! Memory semantics and retrieval policy; persistence remains a Runtime port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod audit;
mod control_plan;
mod control_plan_values;
mod control_plane;
mod control_snapshot;
mod control_snapshot_values;
mod erasure;
mod feedback_quality;
mod hypothesis;
mod lifecycle;
mod maintenance;
mod observation;
mod promotion;
mod query;
mod recall;
mod recall_quality;
mod values;
mod write;

pub use audit::{
    audit_memory, MemoryAuditAction, MemoryAuditEntry, MemoryAuditPolicy, MemoryAuditReport,
    MemoryContradiction,
};
pub use control_plan::prepare_memory_import;
pub use control_plan_values::{
    MemoryAuthorizedScope, MemoryCurrentEntry, MemoryIdentityAllocation, MemoryImportOperation,
    MemoryImportPlan,
};
pub use control_plane::{
    parse_memory_document, MemoryControlDocument, MemoryControlError, MemoryDocumentLimits,
    MemoryRecordRef,
};
pub use control_snapshot::{parse_memory_snapshot, project_memory_snapshot};
pub use control_snapshot_values::{
    MemorySnapshot, MemorySnapshotEntry, MemorySnapshotFile, MemorySnapshotLimits,
    MemorySnapshotManifest,
};
pub use erasure::{
    record_memory_erasure, ErasureDisposition, ErasureTargetKind, ErasureTargetStatus,
    MemoryErasureReceipt, MemoryErasureRequest, MemoryErasureTarget, MemoryErasureTargetResult,
};
pub use feedback_quality::{
    evaluate_recall_feedback_quality, RecallFeedbackOutcome, RecallFeedbackQualityRequest,
    RecallFeedbackQualitySummary, RecallFeedbackRow,
};
pub use hypothesis::{
    import_m0_classification, ImportedMemoryClassification, MemoryAuthority,
    MemoryAuthorityBinding, MemoryRole, MemoryScopeBinding, MemoryScopeClass, MemoryType,
    MemoryTypeDescriptor, MemoryTypeRegistry,
};
pub use lifecycle::{EvidenceTally, HypothesisState, LifecycleEvent, MemoryLifecycle};
pub use maintenance::{
    advance_distillation, decide_candidate, AdmissionAssessment, CandidateStability,
    DistillationWatermark, MaintenanceNoopCode, MemoryCandidate, MemoryCandidateIntent,
    MemoryCandidateSource, MemoryMaintenanceDecision, WatermarkDisposition,
};
pub use observation::{
    reduce_observation, MemoryObligation, MemoryObservation, ObservationEvidence,
    ObservationEvidenceKind, ObservationReduction, ObservationVerdict, ScopeNarrowingCandidate,
};
pub use promotion::{
    complete_memory_promotion, request_memory_promotion, MemoryPromotionPolicy,
    MemoryPromotionReceipt, MemoryPromotionRequest,
};
pub use query::{
    retrieve_memory, MemoryMatch, MemoryPurpose, MemoryQuery, MemoryRetrieval, MemoryScore,
};
pub use recall::{
    select_recall, MemoryRecallCandidate, RecallExploration, RecallProduct, RecallScore,
    RecallSelection, RecallSelectionItem, RecallSelectionKind, RecallSelectionRequest,
};
pub use recall_quality::{
    evaluate_recall_quality, RecallQualityCase, RecallQualityIdentity, RecallQualityRatio,
    RecallQualitySummary,
};

pub use values::{
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryScope,
    MemorySensitivity, MemoryStatus,
};
pub use write::{
    MemoryCommit, MemoryProposal, MemoryRecord, MemoryState, MemorySupersession, MemoryTombstone,
    MemoryWriteOutcome,
};
