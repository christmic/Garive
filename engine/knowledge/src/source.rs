use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::values::{valid_digest, valid_text, MAX_TEXT_BYTES};
use crate::{
    CitationScheme, ContentBinding, KnowledgeError, KnowledgeErrorCode, KnowledgeQueryMode,
    KnowledgeSourceKind, KnowledgeTrustClass,
};

/// Exact source descriptor frozen into an effective Agent snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeSourceDescriptor {
    source_id: String,
    source_revision: String,
    kind: KnowledgeSourceKind,
    content_domain: String,
    trust_class: KnowledgeTrustClass,
    supported_query_modes: Vec<KnowledgeQueryMode>,
    freshness_policy_digest: String,
    citation_scheme: CitationScheme,
    capability_metadata_digest: String,
}
impl KnowledgeSourceDescriptor {
    /// Validates an exact portable source descriptor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_id: impl Into<String>,
        source_revision: impl Into<String>,
        kind: KnowledgeSourceKind,
        content_domain: impl Into<String>,
        trust_class: KnowledgeTrustClass,
        supported_query_modes: Vec<KnowledgeQueryMode>,
        freshness_policy_digest: impl Into<String>,
        citation_scheme: CitationScheme,
        capability_metadata_digest: impl Into<String>,
    ) -> Result<Self, KnowledgeError> {
        let value = Self {
            source_id: source_id.into(),
            source_revision: source_revision.into(),
            kind,
            content_domain: content_domain.into(),
            trust_class,
            supported_query_modes,
            freshness_policy_digest: freshness_policy_digest.into(),
            citation_scheme,
            capability_metadata_digest: capability_metadata_digest.into(),
        };
        if !valid_id(&value.source_id)
            || !valid_id(&value.source_revision)
            || !valid_text(&value.content_domain, MAX_TEXT_BYTES)
            || value.supported_query_modes.is_empty()
            || !value
                .supported_query_modes
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            || !valid_digest(&value.freshness_policy_digest)
            || !valid_digest(&value.capability_metadata_digest)
        {
            Err(KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))
        } else {
            Ok(value)
        }
    }
    /// Returns source identity.
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    /// Returns exact source revision.
    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }
    /// Returns declared trust class.
    pub const fn trust_class(&self) -> KnowledgeTrustClass {
        self.trust_class
    }
    /// Returns declared citation scheme.
    pub const fn citation_scheme(&self) -> CitationScheme {
        self.citation_scheme
    }
    /// Returns whether the source supports the mode.
    pub fn supports(&self, mode: KnowledgeQueryMode) -> bool {
        self.supported_query_modes.contains(&mode)
    }
}

/// Sanitized exact citation binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Citation {
    locator_kind: CitationScheme,
    locator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical_uri: Option<String>,
    content_digest: String,
}
impl Citation {
    /// Validates a bounded sanitized citation.
    pub fn new(
        locator_kind: CitationScheme,
        locator: impl Into<String>,
        title: Option<String>,
        canonical_uri: Option<String>,
        content_digest: impl Into<String>,
    ) -> Result<Self, KnowledgeError> {
        let value = Self {
            locator_kind,
            locator: locator.into(),
            title,
            canonical_uri,
            content_digest: content_digest.into(),
        };
        if !valid_text(&value.locator, MAX_TEXT_BYTES)
            || value
                .title
                .as_deref()
                .is_some_and(|v| !valid_text(v, MAX_TEXT_BYTES))
            || value
                .canonical_uri
                .as_deref()
                .is_some_and(|v| !valid_text(v, MAX_TEXT_BYTES))
            || !valid_digest(&value.content_digest)
        {
            Err(KnowledgeError::new(KnowledgeErrorCode::CitationInvalid))
        } else {
            Ok(value)
        }
    }
    /// Returns citation locator scheme.
    pub const fn locator_kind(&self) -> CitationScheme {
        self.locator_kind
    }
    /// Returns sanitized locator.
    pub fn locator(&self) -> &str {
        &self.locator
    }
    /// Returns optional title.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    /// Returns optional canonical URI.
    pub fn canonical_uri(&self) -> Option<&str> {
        self.canonical_uri.as_deref()
    }
    /// Returns digest bound by the citation.
    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }
}

/// Freshness classification of returned evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeFreshness {
    /// Revalidated for this request.
    Fresh,
    /// Admitted cache result.
    Cached,
    /// Explicit stale evidence.
    Stale,
}

/// One exact attributed evidence chunk returned by Runtime.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeEvidence {
    evidence_id: String,
    source_id: String,
    source_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_snapshot_digest: Option<String>,
    content: ContentBinding,
    content_byte_length: u64,
    citation: Citation,
    retrieved_at_utc: String,
    freshness: KnowledgeFreshness,
    trust_class: KnowledgeTrustClass,
    rank_basis_points: u16,
}
impl KnowledgeEvidence {
    /// Validates exact source, content, citation, size, time, trust and rank bindings.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        evidence_id: impl Into<String>,
        source_id: impl Into<String>,
        source_revision: impl Into<String>,
        source_snapshot_digest: Option<String>,
        content: ContentBinding,
        content_byte_length: u64,
        citation: Citation,
        retrieved_at_utc: impl Into<String>,
        freshness: KnowledgeFreshness,
        trust_class: KnowledgeTrustClass,
        rank_basis_points: u16,
    ) -> Result<Self, KnowledgeError> {
        let value = Self {
            evidence_id: evidence_id.into(),
            source_id: source_id.into(),
            source_revision: source_revision.into(),
            source_snapshot_digest,
            content,
            content_byte_length,
            citation,
            retrieved_at_utc: retrieved_at_utc.into(),
            freshness,
            trust_class,
            rank_basis_points,
        };
        if !valid_id(&value.evidence_id)
            || !valid_id(&value.source_id)
            || !valid_id(&value.source_revision)
            || value
                .source_snapshot_digest
                .as_deref()
                .is_some_and(|v| !valid_digest(v))
            || value.content_byte_length == 0
            || value
                .content
                .inline_utf8()
                .is_some_and(|v| v.len() as u64 != value.content_byte_length)
            || value.citation.content_digest() != value.content.digest()
            || !canonical_utc(&value.retrieved_at_utc)
            || value.rank_basis_points > 10_000
        {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::ContentDigestMismatch,
            ))
        } else {
            Ok(value)
        }
    }
    /// Returns evidence identity.
    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }
    /// Returns source identity.
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    /// Returns exact source revision.
    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }
    /// Returns optional source snapshot digest.
    pub fn source_snapshot_digest(&self) -> Option<&str> {
        self.source_snapshot_digest.as_deref()
    }
    /// Returns exact content.
    pub const fn content(&self) -> &ContentBinding {
        &self.content
    }
    /// Returns verified byte length.
    pub const fn content_byte_length(&self) -> u64 {
        self.content_byte_length
    }
    /// Returns citation.
    pub const fn citation(&self) -> &Citation {
        &self.citation
    }
    /// Returns canonical retrieval time.
    pub fn retrieved_at_utc(&self) -> &str {
        &self.retrieved_at_utc
    }
    /// Returns freshness classification.
    pub const fn freshness(&self) -> KnowledgeFreshness {
        self.freshness
    }
    /// Returns trust class.
    pub const fn trust_class(&self) -> KnowledgeTrustClass {
        self.trust_class
    }
    /// Returns connector rank metadata.
    pub const fn rank_basis_points(&self) -> u16 {
        self.rank_basis_points
    }
}

fn valid_id(value: &str) -> bool {
    valid_text(value, 128)
}
fn canonical_utc(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}
