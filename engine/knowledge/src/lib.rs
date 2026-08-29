//! Knowledge-source contracts, evidence attribution, and retrieval policy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod values;

pub use values::{
    CitationScheme, ContentBinding, KnowledgeError, KnowledgeErrorCode, KnowledgeQueryMode,
    KnowledgeSourceKind, KnowledgeTrustClass,
};
