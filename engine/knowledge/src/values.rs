use serde::Serialize;
use sha2::{Digest, Sha256};

pub(crate) const MAX_TEXT_BYTES: usize = 512;

/// Stable K0 validation, source, connector, citation or durability failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeErrorCode {
    /// A value or relation violates the portable contract.
    InvalidQuery,
    /// The exact source identity is unavailable.
    SourceNotFound,
    /// The requested source revision differs from the resolved descriptor.
    SourceRevisionMismatch,
    /// Runtime policy denied the source.
    SourceDenied,
    /// The source cannot apply one requested filter.
    FilterUnsupported,
    /// The requested freshness cannot be satisfied.
    FreshnessUnavailable,
    /// The connector is temporarily unavailable.
    ConnectorUnavailable,
    /// The connector rejected the request.
    ConnectorRejected,
    /// Dispatch happened without a trustworthy terminal result.
    RetrievalUncertain,
    /// Citation shape or locator is invalid.
    CitationInvalid,
    /// Evidence content and citation digests differ.
    ContentDigestMismatch,
    /// A declared request or result bound was exceeded.
    LimitExceeded,
    /// A required durable commit failed.
    DurabilityFailure,
    /// Persisted Knowledge lifecycle state is impossible.
    CorruptKnowledgeState,
}
impl KnowledgeErrorCode {
    /// Returns the stable portable wire name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidQuery => "invalid_query",
            Self::SourceNotFound => "source_not_found",
            Self::SourceRevisionMismatch => "source_revision_mismatch",
            Self::SourceDenied => "source_denied",
            Self::FilterUnsupported => "filter_unsupported",
            Self::FreshnessUnavailable => "freshness_unavailable",
            Self::ConnectorUnavailable => "connector_unavailable",
            Self::ConnectorRejected => "connector_rejected",
            Self::RetrievalUncertain => "retrieval_uncertain",
            Self::CitationInvalid => "citation_invalid",
            Self::ContentDigestMismatch => "content_digest_mismatch",
            Self::LimitExceeded => "limit_exceeded",
            Self::DurabilityFailure => "durability_failure",
            Self::CorruptKnowledgeState => "corrupt_knowledge_state",
        }
    }
}

/// Typed K0 failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeError {
    code: KnowledgeErrorCode,
}
impl KnowledgeError {
    pub(crate) const fn new(code: KnowledgeErrorCode) -> Self {
        Self { code }
    }
    /// Returns the stable failure classification.
    pub const fn code(&self) -> KnowledgeErrorCode {
        self.code
    }
}

/// Exact inline or Runtime-resolvable content with a SHA-256 binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContentBinding {
    digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_utf8: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
}
impl ContentBinding {
    /// Constructs trusted inline UTF-8 and computes its digest.
    pub fn from_inline(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            digest: sha256(value.as_bytes()),
            inline_utf8: Some(value),
            reference: None,
        }
    }
    /// Validates inline UTF-8 against its supplied digest.
    pub fn inline(
        digest: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, KnowledgeError> {
        let result = Self {
            digest: digest.into(),
            inline_utf8: Some(value.into()),
            reference: None,
        };
        if valid_digest(&result.digest)
            && sha256(result.inline_utf8.as_deref().unwrap().as_bytes()) == result.digest
        {
            Ok(result)
        } else {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::ContentDigestMismatch,
            ))
        }
    }
    /// Validates an opaque Runtime-resolvable content reference.
    pub fn referenced(
        digest: impl Into<String>,
        reference: impl Into<String>,
    ) -> Result<Self, KnowledgeError> {
        let result = Self {
            digest: digest.into(),
            inline_utf8: None,
            reference: Some(reference.into()),
        };
        if valid_digest(&result.digest)
            && valid_text(result.reference.as_deref().unwrap(), MAX_TEXT_BYTES)
        {
            Ok(result)
        } else {
            Err(KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))
        }
    }
    /// Returns the exact content digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
    /// Returns inline content when present.
    pub fn inline_utf8(&self) -> Option<&str> {
        self.inline_utf8.as_deref()
    }
}

/// Portable source category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceKind {
    /// Source-controlled repository.
    Repository,
    /// Maintained documentation corpus.
    Documentation,
    /// Structured dataset.
    Dataset,
    /// Search index.
    SearchIndex,
    /// External service.
    Service,
}
/// Portable query mode.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeQueryMode {
    /// Keyword lookup.
    Keyword,
    /// Semantic retrieval.
    Semantic,
    /// Structured query.
    Structured,
}
/// Declared source trust classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeTrustClass {
    /// Product-curated evidence.
    Curated,
    /// First-party evidence.
    FirstParty,
    /// Third-party evidence.
    ThirdParty,
    /// Untrusted evidence.
    Untrusted,
}
/// Citation locator scheme.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationScheme {
    /// URI plus fragment.
    UriFragment,
    /// Document offset.
    DocumentOffset,
    /// Structured record key.
    RecordKey,
    /// Connector-owned locator.
    OpaqueLocator,
}

pub(crate) fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.trim() == value
}
pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
