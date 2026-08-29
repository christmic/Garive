//! Memory semantics and retrieval policy; persistence remains a Runtime port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod hypothesis;
mod query;
mod values;
mod write;

pub use hypothesis::{
    import_m0_classification, ImportedMemoryClassification, MemoryAuthority,
    MemoryAuthorityBinding, MemoryRole, MemoryScopeBinding, MemoryScopeClass, MemoryType,
    MemoryTypeDescriptor, MemoryTypeRegistry,
};
pub use query::{
    retrieve_memory, MemoryMatch, MemoryPurpose, MemoryQuery, MemoryRetrieval, MemoryScore,
};

pub use values::{
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryScope,
    MemorySensitivity, MemoryStatus,
};
pub use write::{
    MemoryCommit, MemoryProposal, MemoryRecord, MemoryState, MemorySupersession, MemoryTombstone,
    MemoryWriteOutcome,
};
