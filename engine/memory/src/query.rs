use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use chrono::DateTime;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    values::{valid_digest, valid_id, valid_text, MAX_REFERENCE_BYTES},
    write::canonical_utc,
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryRecord,
    MemoryScope, MemorySensitivity, MemoryStatus,
};

const QUERY_CONTRACT: &str = "garive.memory-query";
const CONTRACT_VERSION: u32 = 1;
const MAX_RELEVANCE_BASIS_POINTS: u16 = 10_000;

/// Consumer purpose for one bounded memory query.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPurpose {
    /// Optional attributed evidence for model context.
    Context,
    /// Evidence considered by deterministic planning.
    Planning,
    /// Evidence used to detect a possible revision conflict.
    ConflictCheck,
}

/// Exact deterministic memory query excluding its outer idempotency identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryQuery {
    #[serde(skip)]
    query_id: String,
    namespace_id: String,
    allowed_scopes: Vec<MemoryScope>,
    purpose: MemoryPurpose,
    retriever_revision: String,
    query: ContentBinding,
    through_position: u64,
    as_of_utc: String,
    max_results: u32,
    max_total_bytes: u64,
    include_restricted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    restricted_grant_digest: Option<String>,
}

impl MemoryQuery {
    /// Validates scope order, fixed time, bounds and restricted-grant shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        query_id: impl Into<String>,
        namespace_id: impl Into<String>,
        allowed_scopes: Vec<MemoryScope>,
        purpose: MemoryPurpose,
        retriever_revision: impl Into<String>,
        query: ContentBinding,
        through_position: u64,
        as_of_utc: impl Into<String>,
        max_results: u32,
        max_total_bytes: u64,
        include_restricted: bool,
        restricted_grant_digest: Option<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            query_id: query_id.into(),
            namespace_id: namespace_id.into(),
            allowed_scopes,
            purpose,
            retriever_revision: retriever_revision.into(),
            query,
            through_position,
            as_of_utc: as_of_utc.into(),
            max_results,
            max_total_bytes,
            include_restricted,
            restricted_grant_digest,
        };
        if !valid_id(&value.query_id)
            || !valid_id(&value.namespace_id)
            || value.allowed_scopes.is_empty()
            || !ordered_unique(&value.allowed_scopes)
            || !valid_text(&value.retriever_revision, MAX_REFERENCE_BYTES)
            || !canonical_utc(&value.as_of_utc)
            || value.max_results == 0
            || value.max_total_bytes == 0
            || value.include_restricted != value.restricted_grant_digest.is_some()
            || value
                .restricted_grant_digest
                .as_deref()
                .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Returns the outer query identity.
    pub fn query_id(&self) -> &str {
        &self.query_id
    }
    /// Returns the namespace boundary.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the canonical allowed-scope set.
    pub fn allowed_scopes(&self) -> &[MemoryScope] {
        &self.allowed_scopes
    }
    /// Returns the declared consumer purpose.
    pub const fn purpose(&self) -> MemoryPurpose {
        self.purpose
    }
    /// Returns the exact content query binding.
    pub const fn query(&self) -> &ContentBinding {
        &self.query
    }
    /// Returns the exact retriever revision.
    pub fn retriever_revision(&self) -> &str {
        &self.retriever_revision
    }
    /// Returns the fixed durable prefix.
    pub const fn through_position(&self) -> u64 {
        self.through_position
    }
    /// Returns the frozen query time.
    pub fn as_of_utc(&self) -> &str {
        &self.as_of_utc
    }
    /// Returns the maximum result count.
    pub const fn max_results(&self) -> u32 {
        self.max_results
    }
    /// Returns the maximum exact content bytes.
    pub const fn max_total_bytes(&self) -> u64 {
        self.max_total_bytes
    }
    /// Returns whether restricted results were requested with a frozen grant.
    pub const fn include_restricted(&self) -> bool {
        self.include_restricted
    }
    /// Returns the frozen restricted-grant digest, when present.
    pub fn restricted_grant_digest(&self) -> Option<&str> {
        self.restricted_grant_digest.as_deref()
    }

    /// Computes the RFC 8785 digest over all query semantics except query ID.
    pub fn query_digest(&self) -> Result<String, MemoryError> {
        let value = serde_json::to_value(self)
            .map_err(|_| MemoryError::new(MemoryErrorCode::InvalidMemory))?;
        let preimage =
            json!({"contract": QUERY_CONTRACT, "version": CONTRACT_VERSION, "query": value});
        let bytes = serde_jcs::to_vec(&preimage)
            .map_err(|_| MemoryError::new(MemoryErrorCode::InvalidMemory))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

/// Retriever-owned score and verified content size for one exact revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryScore {
    record_id: String,
    revision_id: String,
    relevance_basis_points: u16,
    content_byte_length: u64,
}

impl MemoryScore {
    /// Validates an exact scored revision reference.
    pub fn new(
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        relevance_basis_points: u16,
        content_byte_length: u64,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            record_id: record_id.into(),
            revision_id: revision_id.into(),
            relevance_basis_points,
            content_byte_length,
        };
        if !valid_id(&value.record_id)
            || !valid_id(&value.revision_id)
            || value.relevance_basis_points > MAX_RELEVANCE_BASIS_POINTS
            || value.content_byte_length == 0
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
}

/// Exact authorized active revision returned by bounded retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryMatch {
    record_id: String,
    revision_id: String,
    kind: MemoryKind,
    content: ContentBinding,
    content_byte_length: u64,
    evidence: Vec<DurableFactReference>,
    relevance_basis_points: u16,
    sensitivity: MemorySensitivity,
}

impl MemoryMatch {
    /// Returns the stable logical record identity.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the immutable revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns exact content.
    pub const fn content(&self) -> &ContentBinding {
        &self.content
    }
    /// Returns the verified byte charge.
    pub const fn content_byte_length(&self) -> u64 {
        self.content_byte_length
    }
    /// Returns ordered evidence.
    pub fn evidence(&self) -> &[DurableFactReference] {
        &self.evidence
    }
    /// Returns the retrieval score.
    pub const fn relevance_basis_points(&self) -> u16 {
        self.relevance_basis_points
    }
    /// Returns the sensitivity class.
    pub const fn sensitivity(&self) -> MemorySensitivity {
        self.sensitivity
    }
    /// Returns the record kind.
    pub const fn kind(&self) -> MemoryKind {
        self.kind
    }
}

/// Completed deterministic retrieval and prefix truncation evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRetrieval {
    /// Ordered authorized matches.
    pub matches: Vec<MemoryMatch>,
    /// Whether an eligible canonical suffix was omitted by a bound.
    pub truncated: bool,
}

/// Filters and orders scored exact revisions under one frozen query.
pub fn retrieve_memory(
    records: &[MemoryRecord],
    scores: &[MemoryScore],
    query: &MemoryQuery,
) -> Result<MemoryRetrieval, MemoryError> {
    let by_identity: BTreeMap<_, _> = records
        .iter()
        .map(|record| ((record.record_id(), record.revision_id()), record))
        .collect();
    let mut seen = BTreeSet::new();
    let mut eligible = Vec::new();
    for score in scores {
        let key = (score.record_id.as_str(), score.revision_id.as_str());
        if !seen.insert(key) {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        let record = by_identity
            .get(&key)
            .ok_or_else(|| MemoryError::new(MemoryErrorCode::CorruptMemoryState))?;
        if record.namespace_id() != query.namespace_id
            || !query.allowed_scopes.contains(record.scope())
            || record.status() != MemoryStatus::Active
            || record.valid_from_position() > query.through_position
            || record
                .expires_at_utc()
                .is_some_and(|expiry| expired(expiry, &query.as_of_utc))
            || record.sensitivity() == MemorySensitivity::Restricted && !query.include_restricted
        {
            continue;
        }
        if record
            .content()
            .inline_utf8()
            .is_some_and(|text| text.len() as u64 != score.content_byte_length)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        eligible.push((*record, score));
    }
    eligible.sort_by_key(|(record, score)| {
        (
            Reverse(score.relevance_basis_points),
            Reverse(record.valid_from_position()),
            record.record_id(),
            record.revision_id(),
        )
    });
    let mut matches = Vec::new();
    let mut bytes = 0_u64;
    let mut truncated = false;
    for (record, score) in eligible {
        let next = bytes.saturating_add(score.content_byte_length);
        if matches.len() == query.max_results as usize || next > query.max_total_bytes {
            truncated = true;
            break;
        }
        bytes = next;
        matches.push(MemoryMatch {
            record_id: record.record_id().into(),
            revision_id: record.revision_id().into(),
            kind: record.kind(),
            content: record.content().clone(),
            content_byte_length: score.content_byte_length,
            evidence: record.evidence().to_vec(),
            relevance_basis_points: score.relevance_basis_points,
            sensitivity: record.sensitivity(),
        });
    }
    Ok(MemoryRetrieval { matches, truncated })
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn expired(expires_at: &str, as_of: &str) -> bool {
    DateTime::parse_from_rfc3339(expires_at).expect("validated record time")
        <= DateTime::parse_from_rfc3339(as_of).expect("validated query time")
}
