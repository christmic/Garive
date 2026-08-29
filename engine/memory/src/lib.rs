//! Memory semantics and retrieval policy; persistence remains a Runtime port.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod values;

pub use values::{
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryScope,
    MemorySensitivity, MemoryStatus,
};
