//! Memory semantics and retrieval policy; persistence remains a Runtime port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod hypothesis;
mod lifecycle;
mod observation;
mod query;
mod recall;
mod values;
mod write;

pub use hypothesis::{
    import_m0_classification, ImportedMemoryClassification, MemoryAuthority,
    MemoryAuthorityBinding, MemoryRole, MemoryScopeBinding, MemoryScopeClass, MemoryType,
    MemoryTypeDescriptor, MemoryTypeRegistry,
};
pub use lifecycle::{EvidenceTally, HypothesisState, LifecycleEvent, MemoryLifecycle};
pub use observation::{
    reduce_observation, MemoryObligation, MemoryObservation, ObservationEvidence,
    ObservationEvidenceKind, ObservationReduction, ObservationVerdict, ScopeNarrowingCandidate,
};
pub use query::{
    retrieve_memory, MemoryMatch, MemoryPurpose, MemoryQuery, MemoryRetrieval, MemoryScore,
};
pub use recall::{
    select_recall, MemoryRecallCandidate, RecallExploration, RecallProduct, RecallScore,
    RecallSelection, RecallSelectionItem, RecallSelectionKind, RecallSelectionRequest,
};

pub use values::{
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryScope,
    MemorySensitivity, MemoryStatus,
};
pub use write::{
    MemoryCommit, MemoryProposal, MemoryRecord, MemoryState, MemorySupersession, MemoryTombstone,
    MemoryWriteOutcome,
};
