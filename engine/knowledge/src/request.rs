use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::values::{valid_digest, valid_text, MAX_TEXT_BYTES};
use crate::{
    ContentBinding, KnowledgeError, KnowledgeErrorCode, KnowledgeQueryMode,
    KnowledgeSourceDescriptor,
};

const REQUEST_CONTRACT: &str = "garive.knowledge-request";
const CONTRACT_VERSION: u32 = 1;

/// Portable strict filter operator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeFilterOperator {
    /// Exact equality.
    Equal,
    /// Strict less-than relation.
    LessThan,
    /// Inclusive less-than relation.
    LessThanOrEqual,
    /// Strict greater-than relation.
    GreaterThan,
    /// Inclusive greater-than relation.
    GreaterThanOrEqual,
}

/// Strict I-JSON filter value subset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum KnowledgeFilterValue {
    /// JSON null.
    Null,
    /// JSON boolean.
    Boolean(bool),
    /// Exact signed integer.
    Integer(i64),
    /// Bounded UTF-8 string.
    String(String),
}

/// One ordered portable source filter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeFilter {
    field: String,
    operator: KnowledgeFilterOperator,
    value: KnowledgeFilterValue,
}
impl KnowledgeFilter {
    /// Validates a bounded filter field and value.
    pub fn new(
        field: impl Into<String>,
        operator: KnowledgeFilterOperator,
        value: KnowledgeFilterValue,
    ) -> Result<Self, KnowledgeError> {
        let value = Self {
            field: field.into(),
            operator,
            value,
        };
        if !valid_text(&value.field, MAX_TEXT_BYTES)
            || matches!(&value.value, KnowledgeFilterValue::String(text) if !valid_text(text, MAX_TEXT_BYTES))
        {
            Err(KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))
        } else {
            Ok(value)
        }
    }
    /// Returns the portable field name.
    pub fn field(&self) -> &str {
        &self.field
    }
    /// Returns the exact portable operator.
    pub const fn operator(&self) -> KnowledgeFilterOperator {
        self.operator
    }
    /// Returns the exact portable value.
    pub const fn value(&self) -> &KnowledgeFilterValue {
        &self.value
    }
}

/// Exact request freshness requirement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FreshnessRequirement {
    /// An admitted cached result may satisfy the request.
    CachedAllowed,
    /// The connector must revalidate freshness.
    Revalidate,
    /// Only one exact source snapshot may satisfy the request.
    ExactSnapshot {
        /// Required exact snapshot digest.
        snapshot_digest: String,
    },
}
impl FreshnessRequirement {
    fn valid(&self) -> bool {
        match self {
            Self::ExactSnapshot { snapshot_digest } => valid_digest(snapshot_digest),
            Self::CachedAllowed | Self::Revalidate => true,
        }
    }
}

/// Exact bounded retrieval request excluding connector configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct KnowledgeRequest {
    #[serde(skip)]
    request_id: String,
    source_id: String,
    source_revision: String,
    mode: KnowledgeQueryMode,
    query: ContentBinding,
    filters: Vec<KnowledgeFilter>,
    through_position: u64,
    max_chunks: u32,
    max_total_bytes: u64,
    deadline_budget_ms: u64,
    freshness_requirement: FreshnessRequirement,
}
impl KnowledgeRequest {
    /// Validates exact source, ordered unique filters and all non-zero bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: impl Into<String>,
        source_id: impl Into<String>,
        source_revision: impl Into<String>,
        mode: KnowledgeQueryMode,
        query: ContentBinding,
        filters: Vec<KnowledgeFilter>,
        through_position: u64,
        max_chunks: u32,
        max_total_bytes: u64,
        deadline_budget_ms: u64,
        freshness_requirement: FreshnessRequirement,
    ) -> Result<Self, KnowledgeError> {
        let value = Self {
            request_id: request_id.into(),
            source_id: source_id.into(),
            source_revision: source_revision.into(),
            mode,
            query,
            filters,
            through_position,
            max_chunks,
            max_total_bytes,
            deadline_budget_ms,
            freshness_requirement,
        };
        let fields: BTreeSet<_> = value.filters.iter().map(KnowledgeFilter::field).collect();
        if !valid_id(&value.request_id)
            || !valid_id(&value.source_id)
            || !valid_id(&value.source_revision)
            || fields.len() != value.filters.len()
            || value.max_chunks == 0
            || value.max_total_bytes == 0
            || value.deadline_budget_ms == 0
            || !value.freshness_requirement.valid()
        {
            Err(KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))
        } else {
            Ok(value)
        }
    }
    /// Validates exact descriptor identity and supported query mode.
    pub fn validate_source(
        &self,
        source: &KnowledgeSourceDescriptor,
    ) -> Result<(), KnowledgeError> {
        if self.source_id != source.source_id() {
            Err(KnowledgeError::new(KnowledgeErrorCode::SourceNotFound))
        } else if self.source_revision != source.source_revision() {
            Err(KnowledgeError::new(
                KnowledgeErrorCode::SourceRevisionMismatch,
            ))
        } else if !source.supports(self.mode) {
            Err(KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))
        } else {
            Ok(())
        }
    }
    /// Computes RFC 8785 SHA-256 over all semantics except request ID.
    pub fn request_digest(&self) -> Result<String, KnowledgeError> {
        let request = serde_json::to_value(self)
            .map_err(|_| KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))?;
        let bytes = serde_jcs::to_vec(
            &json!({"contract":REQUEST_CONTRACT,"version":CONTRACT_VERSION,"request":request}),
        )
        .map_err(|_| KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    /// Returns canonical filter-array content for the durable requested fact.
    pub fn filters_binding(&self) -> Result<ContentBinding, KnowledgeError> {
        let bytes = serde_jcs::to_vec(&self.filters)
            .map_err(|_| KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| KnowledgeError::new(KnowledgeErrorCode::InvalidQuery))?;
        Ok(ContentBinding::from_inline(text))
    }
    /// Returns request identity.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    /// Returns source identity.
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    /// Returns exact source revision.
    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }
    /// Returns query mode.
    pub const fn mode(&self) -> KnowledgeQueryMode {
        self.mode
    }
    /// Returns query content.
    pub const fn query(&self) -> &ContentBinding {
        &self.query
    }
    /// Returns the ordered portable filters.
    pub fn filters(&self) -> &[KnowledgeFilter] {
        &self.filters
    }
    /// Returns fixed durable prefix.
    pub const fn through_position(&self) -> u64 {
        self.through_position
    }
    /// Returns maximum chunks.
    pub const fn max_chunks(&self) -> u32 {
        self.max_chunks
    }
    /// Returns maximum total evidence bytes.
    pub const fn max_total_bytes(&self) -> u64 {
        self.max_total_bytes
    }
    /// Returns deadline budget in milliseconds.
    pub const fn deadline_budget_ms(&self) -> u64 {
        self.deadline_budget_ms
    }
    /// Returns exact freshness requirement.
    pub const fn freshness_requirement(&self) -> &FreshnessRequirement {
        &self.freshness_requirement
    }
}

fn valid_id(value: &str) -> bool {
    valid_text(value, 128)
}
