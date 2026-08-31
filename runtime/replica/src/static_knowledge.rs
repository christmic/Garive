//! Deterministic read-only connector over an explicitly constructed document set.

use std::{sync::Arc, time::SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use garive_knowledge::{
    Citation, CitationScheme, ContentBinding, FreshnessRequirement, KnowledgeEvidence,
    KnowledgeFreshness, KnowledgeQueryMode, KnowledgeSourceDescriptor,
};
use sha2::{Digest, Sha256};

use crate::{KnowledgeConnector, KnowledgeConnectorFuture, KnowledgeConnectorOutcome};

/// Stable constructor failure for an explicit static Knowledge source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticKnowledgeError {
    /// A document identity, content value, order, source, or bound is invalid.
    InvalidConfiguration,
}

/// Immutable bounded document admitted to one static Knowledge source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticKnowledgeDocument {
    document_id: String,
    title: Option<String>,
    content: ContentBinding,
}

impl StaticKnowledgeDocument {
    /// Constructs one inline document under an explicit content-byte bound.
    pub fn new(
        document_id: impl Into<String>,
        title: Option<String>,
        content: impl Into<String>,
        max_content_bytes: usize,
    ) -> Result<Self, StaticKnowledgeError> {
        let document_id = document_id.into();
        let content = content.into();
        if !valid_text(&document_id, 128)
            || title
                .as_deref()
                .is_some_and(|value| !valid_text(value, 512))
            || content.is_empty()
            || max_content_bytes == 0
            || content.len() > max_content_bytes
        {
            return Err(StaticKnowledgeError::InvalidConfiguration);
        }
        Ok(Self {
            document_id,
            title,
            content: ContentBinding::from_inline(content),
        })
    }

    /// Returns the stable source-local document identity.
    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    fn content_byte_length(&self) -> usize {
        self.content
            .inline_utf8()
            .map_or(0, |content| content.len())
    }
}

/// Backend-owned observation clock for attributed connector evidence.
pub trait KnowledgeConnectorClock: Send + Sync {
    /// Returns one canonical RFC 3339 UTC timestamp.
    fn recorded_at(&self) -> String;
}

/// Shipping wall-clock implementation used by local Desktop composition.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemKnowledgeConnectorClock;

impl KnowledgeConnectorClock for SystemKnowledgeConnectorClock {
    fn recorded_at(&self) -> String {
        let now: DateTime<Utc> = SystemTime::now().into();
        now.to_rfc3339_opts(SecondsFormat::AutoSi, true)
    }
}

/// Exact read-only Keyword connector over immutable constructed documents.
pub struct StaticKnowledgeConnector {
    source: KnowledgeSourceDescriptor,
    source_snapshot_digest: String,
    documents: Vec<StaticKnowledgeDocument>,
    clock: Arc<dyn KnowledgeConnectorClock>,
}

impl StaticKnowledgeConnector {
    /// Validates source semantics, canonical document order, and collection bound.
    pub fn new(
        source: KnowledgeSourceDescriptor,
        source_snapshot_digest: impl Into<String>,
        documents: Vec<StaticKnowledgeDocument>,
        max_documents: usize,
        max_total_document_bytes: usize,
        clock: Arc<dyn KnowledgeConnectorClock>,
    ) -> Result<Self, StaticKnowledgeError> {
        let source_snapshot_digest = source_snapshot_digest.into();
        let total_document_bytes = documents.iter().try_fold(0_usize, |total, document| {
            total.checked_add(document.content_byte_length())
        });
        if source.citation_scheme() != CitationScheme::RecordKey
            || !source.supports(KnowledgeQueryMode::Keyword)
            || !valid_digest(&source_snapshot_digest)
            || max_documents == 0
            || max_total_document_bytes == 0
            || documents.is_empty()
            || documents.len() > max_documents
            || total_document_bytes.is_none_or(|total| total > max_total_document_bytes)
            || !documents
                .windows(2)
                .all(|pair| pair[0].document_id < pair[1].document_id)
        {
            return Err(StaticKnowledgeError::InvalidConfiguration);
        }
        Ok(Self {
            source,
            source_snapshot_digest,
            documents,
            clock,
        })
    }
}

impl KnowledgeConnector for StaticKnowledgeConnector {
    fn retrieve<'a>(
        &'a self,
        source: &'a KnowledgeSourceDescriptor,
        request: &'a garive_knowledge::KnowledgeRequest,
    ) -> KnowledgeConnectorFuture<'a> {
        Box::pin(async move {
            if source != &self.source || request.validate_source(source).is_err() {
                return KnowledgeConnectorOutcome::Rejected;
            }
            if !request.filters().is_empty() {
                return KnowledgeConnectorOutcome::FilterUnsupported;
            }
            if matches!(
                request.freshness_requirement(),
                FreshnessRequirement::ExactSnapshot { snapshot_digest }
                    if snapshot_digest != &self.source_snapshot_digest
            ) {
                return KnowledgeConnectorOutcome::FreshnessUnavailable;
            }
            let Some(query) = request.query().inline_utf8() else {
                return KnowledgeConnectorOutcome::Rejected;
            };
            let terms = keyword_terms(query);
            if terms.is_empty() {
                return KnowledgeConnectorOutcome::Completed {
                    evidence: Vec::new(),
                    connector_order_stable: false,
                };
            }
            let recorded_at = self.clock.recorded_at();
            let mut evidence = Vec::new();
            for document in &self.documents {
                let content = document.content.inline_utf8().expect("constructed inline");
                let normalized = content.to_ascii_lowercase();
                let matches = terms
                    .iter()
                    .filter(|term| normalized.contains(term.as_str()))
                    .count();
                if matches == 0 {
                    continue;
                }
                let rank = u16::try_from((matches * 10_000) / terms.len()).unwrap_or(10_000);
                let citation = match Citation::new(
                    CitationScheme::RecordKey,
                    &document.document_id,
                    document.title.clone(),
                    None,
                    document.content.digest(),
                ) {
                    Ok(value) => value,
                    Err(_) => return KnowledgeConnectorOutcome::Rejected,
                };
                let item = KnowledgeEvidence::new(
                    evidence_id(source, &document.document_id),
                    source.source_id(),
                    source.source_revision(),
                    Some(self.source_snapshot_digest.clone()),
                    document.content.clone(),
                    match u64::try_from(content.len()) {
                        Ok(value) => value,
                        Err(_) => return KnowledgeConnectorOutcome::Rejected,
                    },
                    citation,
                    &recorded_at,
                    KnowledgeFreshness::Fresh,
                    source.trust_class(),
                    rank,
                );
                match item {
                    Ok(value) => evidence.push(value),
                    Err(_) => return KnowledgeConnectorOutcome::Rejected,
                }
            }
            KnowledgeConnectorOutcome::Completed {
                evidence,
                connector_order_stable: false,
            }
        })
    }
}

fn keyword_terms(query: &str) -> Vec<String> {
    let mut terms = query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() >= 2)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms
}

fn evidence_id(source: &KnowledgeSourceDescriptor, document_id: &str) -> String {
    let value = format!(
        "{}\0{}\0{document_id}",
        source.source_id(),
        source.source_revision()
    );
    format!("evidence-{:x}", Sha256::digest(value.as_bytes()))
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= max_bytes && value.trim() == value
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
