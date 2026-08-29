//! Knowledge-source contracts, evidence attribution, and retrieval policy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod source;
mod values;

pub use source::{Citation, KnowledgeEvidence, KnowledgeFreshness, KnowledgeSourceDescriptor};

pub use values::{
    CitationScheme, ContentBinding, KnowledgeError, KnowledgeErrorCode, KnowledgeQueryMode,
    KnowledgeSourceKind, KnowledgeTrustClass,
};
