//! Knowledge-source contracts, evidence attribution, and retrieval policy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod request;
mod result;
mod source;
mod values;

pub use request::{
    FreshnessRequirement, KnowledgeFilter, KnowledgeFilterOperator, KnowledgeFilterValue,
    KnowledgeRequest,
};
pub use result::{complete_knowledge, KnowledgeCompleted};
pub use source::{Citation, KnowledgeEvidence, KnowledgeFreshness, KnowledgeSourceDescriptor};

pub use values::{
    CitationScheme, ContentBinding, KnowledgeError, KnowledgeErrorCode, KnowledgeQueryMode,
    KnowledgeSourceKind, KnowledgeTrustClass,
};
