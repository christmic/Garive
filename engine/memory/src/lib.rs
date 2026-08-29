//! Memory semantics and retrieval policy; persistence remains a Runtime port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod values;
mod write;

pub use values::{
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryScope,
    MemorySensitivity, MemoryStatus,
};
pub use write::{
    MemoryCommit, MemoryProposal, MemoryRecord, MemoryState, MemorySupersession, MemoryTombstone,
    MemoryWriteOutcome,
};
